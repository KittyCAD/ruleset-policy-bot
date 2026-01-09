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
        rule_suit::{RuleOutcome, RuleSuite},
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
    let octocrab = create_octocrab(&config)?;

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
            let octocrab = octocrab::Octocrab::builder()
                .app(AppId::from(id), key)
                .build()?
                .installation(InstallationId::from(installation_id as u64))?;

            octocrab
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

        let prs: Option<Vec<PullRequest>> = octocrab
            .commits(github_org, repository_name)
            .associated_pull_requests(PullRequestTarget::Sha(full_result.after_sha.clone()))
            .send()
            .await
            .map(|page: Page<PullRequest>| page.items)
            .ok();

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
            .and_then(|prs| {
                if prs.len() == 1 {
                    prs.into_iter().next()
                } else {
                    None
                }
            });

        //suite_data.rule_evaluations.
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
    let max_ammann = slack.get_user_by_email("max.ammann@zoo.dev").await?;

    let slack_actor = suite_data
        .get_slack_actor(slack, max_ammann.clone(), bot)
        .await?;

    let content = suite_data.build_soc2_notification(&slack_actor, &pr, asset_level, config);

    // Send as DM or to channel based on level
    let call_out = suite_data.call_out_violation(asset_level, resulting_commit, pr, config);

    let soc2_channel = &config.slack_soc2_channel;

    if call_out {
        if let Err(e) = slack
            .post_message(
                SlackChannelId::new(soc2_channel.to_string()),
                content.clone(),
            )
            .await
        {
            return Err(anyhow!("posting a slack message failed: {e}"));
        }
    }

    // Send to actor
    if let Err(e) = slack
        .post_message(
            SlackChannelId::new(slack_actor.id.0.clone()),
            content.clone(),
        )
        .await
    {
        return Err(anyhow!("posting a slack message failed: {e}"));
    }

    // Also send to Max Ammann
    if let Err(e) = slack
        .post_message(SlackChannelId::new(max_ammann.id.0), content)
        .await
    {
        return Err(anyhow!("posting a slack message failed: {e}"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json;

    use crate::soc2::rule_suit::RuleSuite;

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
}
