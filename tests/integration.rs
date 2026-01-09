use async_trait::async_trait;
use chrono::DateTime;
use ruleset_policy_bot::soc2::asset_level::AssetLevel;
use ruleset_policy_bot::soc2::process_rule_suites;
use ruleset_policy_bot::{
    BotConfig, GitHubAppCredentials, GitHubAppInstallation, GitHubAuth, GithubRuleSuiteEvent,
    NewGithubRuleSuiteEvent, RulesetBot, SlackClient, User,
};
use slack_morphism::{SlackChannelId, SlackMessageContent, SlackUser, SlackUserFlags, SlackUserId};
use std::cell::RefCell;
use std::sync::Mutex;

struct MockRulesetBot {
    events: Mutex<RefCell<Vec<NewGithubRuleSuiteEvent>>>,
}

#[async_trait]
impl RulesetBot for MockRulesetBot {
    async fn find_rule_suite_by_github_id(
        &self,
        github_id: &str,
    ) -> anyhow::Result<Option<GithubRuleSuiteEvent>> {
        Ok(None)
    }

    async fn create_rule_suite_event(&self, event: NewGithubRuleSuiteEvent) -> anyhow::Result<()> {
        self.events
            .lock()
            .as_ref()
            .expect("should not be locked")
            .borrow_mut()
            .push(event.clone());
        println!("Created rule suite event: {:?}", event.github_id);
        Ok(())
    }

    async fn find_unnotified_rule_suites(
        &self,
        repository_full_name: &str,
    ) -> anyhow::Result<Vec<GithubRuleSuiteEvent>> {
        println!(
            "Finding unnotified rule suites for {}",
            repository_full_name
        );
        Ok(self
            .events
            .lock()
            .as_ref()
            .expect("should not be locked")
            .borrow()
            .iter()
            .map(|event| GithubRuleSuiteEvent {
                id: 123,
                github_id: event.github_id.clone(),
                repository_full_name: event.repository_full_name.clone(),
                event_data: event.event_data.clone(),
                resulting_commit: event.resulting_commit.clone(),
                prs: event.prs.clone(),
                notified: event.notified,
                created_at: DateTime::from_timestamp(0, 0).expect("valid timestamp"),
                updated_at: DateTime::from_timestamp(0, 0).expect("valid timestamp"),
            })
            .collect())
    }

    async fn mark_rule_suite_notified(&self, id: i32) -> anyhow::Result<()> {
        println!("Marked rule suite {} as notified", id);
        Ok(())
    }

    async fn get_email_by_github_username(
        &self,
        github_username: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(Some("max.ammann@zoo.dev".to_string()))
    }
}

struct MockSlackClient {
    slack_messages: Mutex<RefCell<Vec<SlackMessageContent>>>,
}

#[async_trait]
impl SlackClient for MockSlackClient {
    async fn get_user_by_email(&self, email: &str) -> anyhow::Result<SlackUser> {
        Ok(SlackUser::new(
            SlackUserId(email.to_string()),
            SlackUserFlags::new(),
        ))
    }

    async fn post_message(
        &self,
        channel_id: SlackChannelId,
        content: SlackMessageContent,
    ) -> anyhow::Result<()> {
        println!("Posted message to channel {}", channel_id);
        self.slack_messages
            .lock()
            .as_ref()
            .expect("should not be locked")
            .borrow_mut()
            .push(content);
        Ok(())
    }
}

#[tokio::test]
async fn test_updating_rule_suites() {
    let bot = MockRulesetBot {
        events: Mutex::new(RefCell::new(vec![])),
    };
    let slack_client = MockSlackClient {
        slack_messages: Mutex::new(RefCell::new(vec![])),
    };
    process_rule_suites(
        &bot,
        &BotConfig {
            github_org: "KittyCAD".to_string(),
            github_web_base_url: "https://github.com/".to_string(),
            slack_soc2_channel: "".to_string(),
            review_requirement_ruleset_id: None,
            block_force_push_ruleset_id: None,
            codeowners_ruleset_id: None,
            in_scope_asset_level: AssetLevel::Playground..=AssetLevel::Playground,
            github_auth: GitHubAuth::Token(std::env::var("GH_TOKEN").unwrap()),
        },
        &slack_client,
        "KittyCAD/ruleset-policy-bot",
        "ruleset-policy-bot",
    )
    .await
    .unwrap();

    insta::assert_debug_snapshot!(
        slack_client
            .slack_messages
            .lock()
            .as_ref()
            .expect("should not be locked")
            .borrow()
            .first()
    );
}
