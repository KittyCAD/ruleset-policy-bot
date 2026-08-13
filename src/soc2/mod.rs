pub mod asset_level;
pub mod rule_suit;

use anyhow::{Context, Result, anyhow};
use octocrab::{
    Octocrab, Page,
    commits::PullRequestTarget,
    models::{AppId, InstallationId, pulls::PullRequest, repos::RepoCommit},
};
use slack_morphism::SlackChannelId;

use crate::{
    BotConfig, GitHubAuth, NewGithubRuleSuiteEvent, RulesetBot, SlackClient,
    soc2::{
        asset_level::{AssetLevel, CustomPropertyExt},
        rule_suit::{RuleOutcome, RuleSuite, github_test_merge_parent_shas},
    },
};

#[tracing::instrument(skip(bot, config, slack))]
pub async fn process_rule_suites(
    bot: &dyn RulesetBot,
    config: &BotConfig,
    slack: &dyn SlackClient,
    repository_full_name: &str,
    repository_name: &str,
) -> anyhow::Result<()> {
    let octocrab = create_octocrab(config)?;

    update_rule_suites(
        bot,
        config,
        &octocrab,
        repository_full_name,
        repository_name,
    )
    .await?;
    evaluate_rule_suites(
        bot,
        config,
        slack,
        &octocrab,
        repository_full_name,
        repository_name,
    )
    .await?;
    Ok(())
}

pub fn create_octocrab(config: &BotConfig) -> Result<Octocrab> {
    let octocrab = match &config.github_auth {
        GitHubAuth::Installation(auth_context) => {
            let credentials = &auth_context.credentials;
            let installation_id = auth_context.installation_id;

            let key = jsonwebtoken::EncodingKey::from_rsa_pem(credentials.private_key.as_bytes())?;

            let id: u64 = credentials.app_id.parse()?;

            octocrab::Octocrab::builder()
                .app(AppId::from(id), key)
                .build()?
                .installation(InstallationId::from(installation_id as u64))?
        }
        GitHubAuth::Token(token) => octocrab::Octocrab::builder()
            .personal_token(token.to_string())
            .build()?,
    };
    Ok(octocrab)
}

#[tracing::instrument(skip(bot, config, octocrab))]
async fn update_rule_suites(
    bot: &dyn RulesetBot,
    config: &BotConfig,
    octocrab: &Octocrab,
    repository_full_name: &str,
    repository_name: &str,
) -> anyhow::Result<()> {
    // Update rule suites in the DB
    // We are hoping here that the rule suites are already available via the API. If not they will get fetched with the next repo event.

    let github_org = &config.github_org;

    // https://docs.github.com/en/rest/repos/rule-suites?apiVersion=2022-11-28#list-repository-rule-suites
    let url = format!("/repos/{repository_full_name}/rulesets/rule-suites");
    let rule_suites: Vec<RuleSuite> = octocrab
        .get(url, None::<&()>)
        .await
        .context("unable to fetch rule suites")?;
    // Process each rule suite.
    for suite in rule_suites {
        if suite.result != RuleOutcome::Bypass {
            continue;
        }

        // Skip rule suites created by bots. Some bots in our org can bypass and commit directly to main.
        if let Some(actor) = suite.actor_name
            && actor.contains("[bot]")
        {
            continue;
        }

        let Ok(full_result): octocrab::Result<RuleSuite> = octocrab
            .get(
                format!(
                    "/repos/{}/rulesets/rule-suites/{}",
                    repository_full_name, suite.id
                ),
                None::<&()>,
            )
            .await
        else {
            tracing::warn!(
                "Failed to fetch full rule suite data for suite ID {}",
                suite.id
            );
            continue;
        };

        let resulting_commit = octocrab
            .commits(github_org, repository_name)
            .get(&full_result.after_sha)
            .await
            .ok();

        let mut prs: Option<Vec<PullRequest>> = octocrab
            .commits(github_org, repository_name)
            .associated_pull_requests(PullRequestTarget::Sha(full_result.after_sha.clone()))
            .send()
            .await
            .map(|page: Page<PullRequest>| page.items)
            .ok();

        if prs.as_ref().is_none_or(Vec::is_empty)
            && let Some((_, head_sha)) = resulting_commit
                .as_ref()
                .and_then(github_test_merge_parent_shas)
        {
            prs = octocrab
                .commits(github_org, repository_name)
                .associated_pull_requests(PullRequestTarget::Sha(head_sha.to_string()))
                .send()
                .await
                .map(|page: Page<PullRequest>| {
                    matching_pull_requests(page.items, head_sha, &full_result.ref_name)
                })
                .ok();
        }

        // Insert rule suite if id does not yet exist.
        let Ok(lookup) = bot
            .find_rule_suite_by_github_id(&suite.id.to_string())
            .await
        else {
            continue;
        };

        if lookup.is_none()
            && let Err(e) = bot
                .create_rule_suite_event(NewGithubRuleSuiteEvent {
                    github_id: suite.id.to_string(),
                    repository_full_name: repository_full_name.to_string(),
                    event_data: serde_json::to_string(&full_result)?,
                    resulting_commit: resulting_commit
                        .and_then(|repo_commit| serde_json::to_string(&repo_commit).ok()),
                    prs: prs.and_then(|prs| serde_json::to_string(&prs).ok()),
                    notified: false,
                })
                .await
        {
            tracing::warn!(
                "Failed to create rule suite event for suite ID {}: {}",
                suite.id,
                e
            );
            continue;
        }
    }

    Ok(())
}

fn matching_pull_requests(
    pull_requests: Vec<PullRequest>,
    head_sha: &str,
    target_ref: &str,
) -> Vec<PullRequest> {
    let Some(base_ref) = target_ref.strip_prefix("refs/heads/") else {
        return Vec::new();
    };

    pull_requests
        .into_iter()
        .filter(|pr| pr.head.sha == head_sha && pr.base.ref_field == base_ref)
        .collect()
}

#[tracing::instrument(skip(bot, config, slack, octocrab))]
pub async fn evaluate_rule_suites(
    bot: &dyn RulesetBot,
    config: &BotConfig,
    slack: &dyn SlackClient,
    octocrab: &Octocrab,
    repository_full_name: &str,
    repository_name: &str,
) -> anyhow::Result<()> {
    let github_org = &config.github_org;
    let props = octocrab
        .list_custom_properties(github_org, repository_name)
        .await?;

    let Some(asset_level) = AssetLevel::get_from_props(&props) else {
        // Ignore repositories without asset level.
        return Ok(());
    };

    if !config.in_scope_asset_level.contains(&asset_level) {
        // Ignore out of scope repos.
        return Ok(());
    }

    // Get all rule suites for the repository that have not yet been notified.
    let rule_suites = bot
        .find_unnotified_rule_suites(repository_full_name)
        .await?;

    if rule_suites.is_empty() {
        return Ok(());
    }

    for suite in rule_suites {
        let suite_data: RuleSuite = serde_json::from_str(&suite.event_data)?;
        let resulting_commit = suite
            .resulting_commit
            .and_then(|json| serde_json::from_str::<RepoCommit>(&json).ok());
        let pr = suite
            .prs
            .and_then(|json| serde_json::from_str::<Vec<PullRequest>>(&json).ok())
            .and_then(|prs| prs.first().cloned());

        send_violation_slack_message(
            slack,
            &suite_data,
            resulting_commit,
            pr,
            asset_level,
            bot,
            config,
        )
        .await?;

        // Update the evaluation result in the DB.
        bot.mark_rule_suite_notified(suite.id).await?;
    }

    Ok(())
}

pub async fn send_violation_slack_message(
    slack: &dyn SlackClient,
    suite_data: &RuleSuite,
    resulting_commit: Option<RepoCommit>,
    pr: Option<PullRequest>,
    asset_level: AssetLevel,
    bot: &dyn RulesetBot,
    config: &BotConfig,
) -> Result<()> {
    let mut suite_data = suite_data.clone();
    if suite_data.suppress_synthetic_merge_linear_history_notification(
        resulting_commit.as_ref(),
        pr.as_ref(),
    ) {
        return Ok(());
    }

    let max_ammann = slack.get_user_by_email("max.ammann@zoo.dev").await?;

    let slack_actor = suite_data
        .get_slack_actor(slack, bot)
        .await?
        .unwrap_or(max_ammann.clone());

    let content = suite_data.build_soc2_notification(&slack_actor, &pr, asset_level, config);

    // Send as DM or to channel based on level
    let call_out = suite_data.call_out_violation(asset_level, resulting_commit, pr, config);

    let soc2_channel = &config.slack_soc2_channel;

    if call_out
        && let Err(e) = slack
            .post_message_channel(
                SlackChannelId::new(soc2_channel.to_string()),
                content.clone(),
            )
            .await
    {
        return Err(anyhow!("posting a slack message failed: {e}"));
    }

    // Send to actor
    if let Err(e) = slack
        .post_message_user(slack_actor.id, content.clone())
        .await
    {
        return Err(anyhow!("posting a slack message failed: {e}"));
    }

    // Also send to Max Ammann
    if let Err(e) = slack.post_message_user(max_ammann.id, content).await {
        return Err(anyhow!("posting a slack message failed: {e}"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use octocrab::models::pulls::PullRequest;
    use serde_json;

    use crate::soc2::{matching_pull_requests, rule_suit::RuleSuite};

    fn pull_request(number: u64, base_ref: &str, base_sha: &str, head_sha: &str) -> PullRequest {
        serde_json::from_value(serde_json::json!({
            "url": format!("https://api.github.com/repos/KittyCAD/modeling-app/pulls/{number}"),
            "id": number,
            "number": number,
            "locked": false,
            "maintainer_can_modify": false,
            "head": {
                "ref": "feature",
                "sha": head_sha
            },
            "base": {
                "ref": base_ref,
                "sha": base_sha
            }
        }))
        .expect("pull request fixture should deserialize")
    }

    /// Load JSON fixture from the `tests/fixtures` directory.
    fn load_fixture(name: &str) -> String {
        let path = format!("tests/fixtures/{name}");
        fs::read_to_string(path).unwrap_or_else(|_| panic!("Fixture {} not found", name))
    }

    #[test]
    fn test_deserialize_rulesuite1() {
        let json_str = load_fixture("rulesuite1.json");

        let parsed: RuleSuite =
            serde_json::from_str(&json_str).expect("Failed to deserialize RuleSuite fixture");

        // Example assertions (adjust based on fixture content)
        assert_eq!(parsed.id, 1023523815);
        assert_eq!(parsed.repository_name, "my_repo");
        assert!(parsed.actor_id.is_some());
    }

    #[test]
    fn test_deserialize_rulesuite2() {
        let json_str = load_fixture("rulesuite2.json");

        let parsed: RuleSuite =
            serde_json::from_str(&json_str).expect("Failed to deserialize RuleSuite fixture");

        // Example assertions (adjust based on fixture content)
        assert_eq!(parsed.id, 1023238279);
        assert_eq!(parsed.repository_name, "modeling-app");
        assert!(parsed.actor_id.is_some());
    }

    #[test]
    fn test_deserialize_rulesuites() {
        let json_str = load_fixture("rulesuites.json");

        let _parsed: Vec<RuleSuite> =
            serde_json::from_str(&json_str).expect("Failed to deserialize RuleSuite fixture");
    }

    #[test]
    fn matches_test_merge_pr_by_head_and_target_ref() {
        let head_sha = "59383581b511a93a9d0fb51a2878fba1ac9dd2da";
        let pull_requests = vec![
            pull_request(
                12831,
                "main",
                "85c12dc51bbd517d5a9b8068e6f36e789e8ef314",
                head_sha,
            ),
            pull_request(
                12982,
                "main",
                "787ae4b9406189327c91c75a9f1f4744b40a2d91",
                "360f8fc350d57662b03f8bd67f55aedf0f236861",
            ),
            pull_request(
                13000,
                "release",
                "787ae4b9406189327c91c75a9f1f4744b40a2d91",
                head_sha,
            ),
        ];

        let matching = matching_pull_requests(pull_requests, head_sha, "refs/heads/main");

        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].number, 12831);
    }
}
