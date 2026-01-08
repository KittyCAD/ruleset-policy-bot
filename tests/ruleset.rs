use async_trait::async_trait;
use ruleset_policy_bot::soc2::process_rule_suites;
use ruleset_policy_bot::{
    BotConfig, GitHubAppCredentials, GitHubAppInstallation, GitHubAuth, GithubRuleSuiteEvent,
    NewGithubRuleSuiteEvent, RulesetBot, SlackClient, User,
};
use slack_morphism::{SlackChannelId, SlackMessageContent, SlackUser};

struct MockRulesetBot;

#[async_trait]
impl RulesetBot for MockRulesetBot {
    async fn find_rule_suite_by_github_id(
        &self,
        github_id: &str,
    ) -> anyhow::Result<Option<GithubRuleSuiteEvent>> {
        Ok(None)
    }

    async fn create_rule_suite_event(&self, event: NewGithubRuleSuiteEvent) -> anyhow::Result<()> {
        Ok(())
    }

    async fn find_unnotified_rule_suites(
        &self,
        repository_full_name: &str,
    ) -> anyhow::Result<Vec<GithubRuleSuiteEvent>> {
        Ok(vec![])
    }

    async fn mark_rule_suite_notified(&self, id: i32) -> anyhow::Result<()> {
        Ok(())
    }

    async fn get_email_by_github_username(
        &self,
        github_username: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(Some("max.ammann@zoo.dev".to_string()))
    }
}

struct MockSlackClient;

#[async_trait]
impl SlackClient for MockSlackClient {
    async fn get_user_by_email(&self, email: &str) -> anyhow::Result<SlackUser> {
        todo!()
    }

    async fn post_message(
        &self,
        channel_id: SlackChannelId,
        content: SlackMessageContent,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn test() {
    process_rule_suites(
        &MockRulesetBot,
        &BotConfig {
            github_org: "".to_string(),
            github_web_base_url: "".to_string(),
            slack_soc2_channel: "".to_string(),
            review_requirement_ruleset_id: None,
            block_force_push_ruleset_id: None,
            codeowners_ruleset_id: None,
            github_auth: GitHubAuth::Token(std::env::var("GH_TOKEN").unwrap()),
        },
        &MockSlackClient,
        "KittyCAD/ruleset-policy-bot",
        "KittyCAD/ruleset-policy-bot",
    )
    .await
    .unwrap();
}
