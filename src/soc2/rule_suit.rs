use std::fmt::{Display, Formatter};

use crate::BotConfig;
use crate::soc2::asset_level::AssetLevel;
use chrono::{DateTime, Utc};
use octocrab::models::{pulls::PullRequest, repos::RepoCommit};
use serde::{Deserialize, Serialize};
use slack_morphism::prelude::*;

#[derive(Debug, Serialize, Deserialize)]
pub struct RuleSuite {
    pub id: i64,

    pub actor_id: Option<i64>,
    pub actor_name: Option<String>,

    pub before_sha: String,
    pub after_sha: String,

    #[serde(rename = "ref")]
    pub ref_name: String,

    pub repository_id: i64,
    pub repository_name: String,

    pub pushed_at: DateTime<Utc>,

    pub result: RuleOutcome,

    pub evaluation_result: Option<RuleOutcome>,

    pub rule_evaluations: Option<Vec<RuleEvaluation>>,
}

impl RuleSuite {
    pub fn call_out_violation(
        &self,
        asset_level: AssetLevel,
        resulting_commit: Option<RepoCommit>,
        pr: Option<PullRequest>,
        config: &BotConfig,
    ) -> bool {
        if config.callout_asset_level.contains(&asset_level) {
            let is_review_force_push_violation =
                self.any(|eval| eval.is_block_force_push_bypass(config));

            if is_review_force_push_violation {
                return true;
            } else {
                // Ignore CO violations for now. Not SOC2 relevant. Send as DM.
                // let is_codeowner_violation = self.any(|eval| eval.is_codeowners_bypass(config));
                // Ignore CIO violations for now. Send as DM.
                // let is_ci_violation = false;

                let is_review_violation =
                    self.any(|eval| eval.is_review_requirement_bypass(config));
                let is_branch_protection =
                    self.any(|eval| match eval.rule_source.evaluated_rule_source() {
                        EvaluatedRuleSource::Ruleset { .. } => false,
                        EvaluatedRuleSource::ProtectedBranch => true,
                        // coulde be a branch protection one
                        EvaluatedRuleSource::Unknown { .. } => true,
                    });

                if is_branch_protection {
                    return true;
                }

                let is_dependabot_pr = resulting_commit
                    .and_then(|commit| commit.author)
                    .map(|author| author.id.0 == 49699333 && author.login == "dependabot[bot]")
                    .unwrap_or(false);
                let is_policy_exception_label = pr
                    .map(|pr| {
                        pr.labels
                            .iter()
                            .flatten()
                            .any(|label| label.name.contains("policy-exception"))
                    })
                    .unwrap_or(false);

                if is_review_violation && !is_dependabot_pr && !is_policy_exception_label {
                    return true;
                }
            }
        }

        false
    }

    /// Returns true if any rule evaluation satisfies the predicate. This ignored successful evaluations.
    pub fn any<F>(&self, predicate: F) -> bool
    where
        F: Fn(&RuleEvaluation) -> bool,
    {
        if self.result != RuleOutcome::Bypass {
            return false;
        }

        self.rule_evaluations
            .as_ref()
            .map(|evals| {
                evals
                    .iter()
                    .any(|eval| eval.result == RuleEvalResult::Fail && predicate(eval))
            })
            .unwrap_or(false)
    }
    pub fn get_commit_url(&self, config: &BotConfig) -> String {
        format!(
            "{base}/{org}/{repo}/commit/{sha}",
            base = config.github_web_base_url,
            org = config.github_org,
            repo = self.repository_name,
            sha = self.after_sha,
        )
    }

    pub async fn get_slack_actor(
        &self,
        slack: &dyn crate::SlackClient,
        db: &dyn crate::RulesetBot,
    ) -> anyhow::Result<Option<SlackUser>> {
        Ok(if let Some(actor) = &self.actor_name {
            let email = db.get_email_by_github_username(actor).await?;

            if let Some(email) = email {
                Some(slack.get_user_by_email(&email).await?)
            } else {
                None
            }
        } else {
            None
        })
    }

    pub fn build_soc2_notification(
        &self,
        slack_actor: &SlackUser,
        pr: &Option<PullRequest>,
        asset_level: AssetLevel,
        config: &BotConfig,
    ) -> SlackMessageContent {
        let is_critical = config.critical_asset_levels.contains(&asset_level)
            && if let Some(rule_evaluations) = &self.rule_evaluations {
                rule_evaluations
                    .iter()
                    .any(|eval| eval.is_critical_violation(config))
            } else {
                false
            };

        let mut blocks: Vec<SlackBlock> = Vec::new();
        blocks.push(
            SlackHeaderBlock {
                block_id: None,
                text: SlackBlockPlainText::from(format!(
                    "{}GitHub Policy Violation",
                    if is_critical {
                        "Critical "
                    } else {
                        "Potential "
                    }
                ))
                .into(),
            }
            .into(),
        );

        let summary = if is_critical {
            format!(
                "<@{}>, please leave a comment in the thread why the below rules were violated.",
                slack_actor.id.0,
            )
        } else {
            format!(
                "<@{}>, please make sure no security policy has been violated. No need to comment.",
                slack_actor.id.0,
            )
        };

        blocks.push(
            SlackSectionBlock {
                block_id: None,
                text: Some(SlackBlockText::MarkDown(SlackBlockMarkDownText::from(
                    summary.clone(),
                ))),
                fields: None,
                accessory: None,
            }
            .into(),
        );

        let actor = format!(
            "*Actor*\n{}",
            self.actor_name.clone().unwrap_or("Unknown".to_string())
        );

        blocks.push(
            SlackSectionBlock {
                block_id: None,
                text: Some(SlackBlockText::MarkDown(SlackBlockMarkDownText::from(
                    actor,
                ))),
                fields: None,
                accessory: None,
            }
            .into(),
        );

        let mut attachments = vec![];

        if let Some(rule_evaluations) = &self.rule_evaluations {
            for evaluation in rule_evaluations {
                if !evaluation.is_failed() {
                    continue;
                }

                let commit_url = self.get_commit_url(config);

                let mut fields: Vec<SlackMessageAttachmentFieldObject> = vec![
                    SlackMessageAttachmentFieldObject {
                        title: Some("Commit".to_string()),
                        value: Some(format!(
                            "<{}|`{}`> in `{}`.",
                            commit_url,
                            &self.after_sha.get(..7).unwrap_or("commit"),
                            self.repository_name
                        )),
                        short: Some(true),
                    },
                    SlackMessageAttachmentFieldObject {
                        title: Some("Sub-type".to_string()),
                        value: Some(format!("*{}*", evaluation.rule_type)),
                        short: Some(true),
                    },
                ];

                if let Some(PullRequest {
                    number,
                    html_url: Some(html_url),
                    ..
                }) = &pr
                {
                    fields.push(SlackMessageAttachmentFieldObject {
                        title: Some("Pull Request".to_string()),
                        value: Some(format!("<{}|#{}>", html_url, number)),
                        short: Some(false),
                    });
                }

                if let Some(details) = &evaluation.details {
                    fields.push(SlackMessageAttachmentFieldObject {
                        title: Some("Details".to_string()),
                        value: Some(details.clone()),
                        short: Some(false),
                    });
                }

                let color = evaluation.attachment_color(config).to_string();

                match evaluation.rule_source.evaluated_rule_source() {
                    EvaluatedRuleSource::Ruleset { name, id } => {
                        fields.push(SlackMessageAttachmentFieldObject {
                            title: Some("Ruleset".to_string()),
                            value: Some(format!(
                                // TODO this url might be broken if its a repo ruleset
                                "<https://github.com/organizations/KittyCAD/settings/rules/{id}|{name}>",
                            )),
                            short: Some(false),
                        });
                    }
                    EvaluatedRuleSource::ProtectedBranch => {
                        fields.push(SlackMessageAttachmentFieldObject {
                            title: Some("Source".to_string()),
                            value: Some("branch protection".to_string()),
                            short: Some(false),
                        });
                    }
                    EvaluatedRuleSource::Unknown { typ, .. } => {
                        fields.push(SlackMessageAttachmentFieldObject {
                            title: Some("Source".to_string()),
                            value: Some(typ.to_string()),
                            short: Some(false),
                        });
                    }
                }

                attachments.push(SlackMessageAttachment {
                    id: None,
                    color: Some(color),
                    fallback: Some("no fallback".to_string()),
                    title: None,
                    fields: Some(fields),
                    mrkdwn_in: Some(vec!["fields".to_string()]),
                    text: None,
                    blocks: None,
                });
            }
        }

        let fallback = format!("{summary}\n\n{self}");

        SlackMessageContent {
            text: Some(fallback),
            blocks: Some(blocks),
            attachments: Some(attachments),
            upload: None,
            files: None,
            reactions: None,
            metadata: None,
        }
    }
}

impl Display for RuleSuite {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.result != RuleOutcome::Bypass {
            return writeln!(f, "Non-bypass rule must not be evaluated.");
        }

        let mut no_failures = true;

        if let Some(rule_evaluations) = &self.rule_evaluations {
            for evaluation in rule_evaluations {
                if evaluation.result != RuleEvalResult::Fail {
                    continue;
                }

                no_failures = false;

                let rule_type = &evaluation.rule_source.typ;
                let sub_type = &evaluation.rule_type;
                let actor = self.actor_name.clone().unwrap_or("unknown".to_string());

                write!(f, "{actor} violated rule (`{sub_type}`)")?;

                if let Some(name) = &evaluation.rule_source.name {
                    if rule_type == "ruleset" {
                        write!(f, " from ruleset `{name}`")?;
                    } else {
                        write!(f, " from `{name}`")?;
                    }
                }

                // Note: Display trait doesn't have access to config, so we use a basic format
                let commit_url = format!(
                    "https://github.com/{}/commit/{}",
                    self.repository_name, self.after_sha
                );
                writeln!(
                    f,
                    " with <{}|`{}`> in `{}`.",
                    commit_url,
                    &self.after_sha.get(..7).unwrap_or("commit"),
                    self.repository_name
                )?;

                if let Some(details) = &evaluation.details {
                    writeln!(f)?;
                    writeln!(f, "{details}")?;
                }
            }
        } else {
            return writeln!(f, "Bypass without rule evaluations.");
        }

        if no_failures {
            writeln!(f, "Bypass with no failures.")?;
        }

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RuleOutcome {
    Pass,
    Fail,
    Bypass,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuleEvaluation {
    pub rule_source: RuleSource,

    pub enforcement: Enforcement,

    pub result: RuleEvalResult,

    pub rule_type: String,

    /// Only available if rule_source.type is "protected_branch"
    pub details: Option<String>,
}

impl RuleEvaluation {
    pub fn attachment_color(&self, config: &BotConfig) -> &'static str {
        if self.is_critical_violation(config) {
            // red
            "#E01E5A"
        } else {
            // orange
            "#ECB22E"
        }
    }

    pub fn is_failed(&self) -> bool {
        self.enforcement == Enforcement::Active && self.result == RuleEvalResult::Fail
    }

    pub fn is_critical_violation(&self, config: &BotConfig) -> bool {
        self.is_review_requirement_bypass(config) || self.is_block_force_push_bypass(config)
    }

    pub fn is_review_requirement_bypass(&self, config: &BotConfig) -> bool {
        self.is_failed()
            && config
                .review_requirement_ruleset_id
                .map(|id| self.rule_source.id == Some(id))
                .unwrap_or(false)
    }

    pub fn is_block_force_push_bypass(&self, config: &BotConfig) -> bool {
        self.is_failed()
            && config
                .block_force_push_ruleset_id
                .map(|id| self.rule_source.id == Some(id))
                .unwrap_or(false)
    }

    pub fn is_codeowners_bypass(&self, config: BotConfig) -> bool {
        self.is_failed()
            && config
                .codeowners_ruleset_id
                .map(|id| self.rule_source.id == Some(id))
                .unwrap_or(false)
    }
}

pub enum EvaluatedRuleSource {
    Ruleset {
        id: i64,
        name: String,
    },
    ProtectedBranch,
    Unknown {
        id: Option<i64>,
        name: Option<String>,
        typ: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuleSource {
    #[serde(rename = "type")]
    pub typ: String,

    pub id: Option<i64>,

    pub name: Option<String>,
}

impl RuleSource {
    pub fn evaluated_rule_source(&self) -> EvaluatedRuleSource {
        match (self.typ.as_str(), self.id, self.name.clone()) {
            ("ruleset", Some(id), Some(name)) => EvaluatedRuleSource::Ruleset { id, name },
            ("protected_branch", _id, _name) => EvaluatedRuleSource::ProtectedBranch,
            (typ, id, name) => EvaluatedRuleSource::Unknown {
                id,
                name,
                typ: typ.to_string(),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Enforcement {
    Active,
    Evaluate,
    #[serde(rename = "deleted ruleset")]
    DeletedRuleset,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum RuleEvalResult {
    Pass,
    Fail,
}
