use async_trait::async_trait;
use chrono::DateTime;
use octocrab::models::pulls::PullRequest;
use octocrab::models::repos::{
    CommitAuthor, CommitObject, RepoCommit, RepoCommitPage, Verification,
};
use octocrab::models::{Author, UserId};
use ruleset_policy_bot::soc2::asset_level::AssetLevel;
use ruleset_policy_bot::soc2::rule_suit::{
    Enforcement, RuleEvalResult, RuleEvaluation, RuleSource, RuleSuite,
};
use ruleset_policy_bot::soc2::{create_octocrab, evaluate_rule_suites, process_rule_suites};
use ruleset_policy_bot::{
    BotConfig, GitHubAppCredentials, GitHubAppInstallation, GitHubAuth, GithubRuleSuiteEvent,
    NewGithubRuleSuiteEvent, RulesetBot, SlackClient, User,
};
use slack_morphism::{SlackChannelId, SlackMessageContent, SlackUser, SlackUserFlags, SlackUserId};
use std::cell::RefCell;
use std::sync::Mutex;
use url::Host::Domain;

const COMMIT: &str = // language=json
    r#"
{
  "url": "https://api.github.com/repos/KittyCAD/ruleset-policy-bot/commits/d6602d2416760fb1bee076fbd895b97e41a0f0f7",
  "sha": "d6602d2416760fb1bee076fbd895b97e41a0f0f7",
  "node_id": "C_kwDOQhN5vdoAKGQ2NjAyZDI0MTY3NjBmYjFiZWUwNzZmYmQ4OTViOTdlNDFhMGYwZjc",
  "html_url": "https://github.com/KittyCAD/ruleset-policy-bot/commit/d6602d2416760fb1bee076fbd895b97e41a0f0f7",
  "comments_url": "https://api.github.com/repos/KittyCAD/ruleset-policy-bot/commits/d6602d2416760fb1bee076fbd895b97e41a0f0f7/comments",
  "commit": {
    "url": "https://api.github.com/repos/KittyCAD/ruleset-policy-bot/git/commits/d6602d2416760fb1bee076fbd895b97e41a0f0f7",
    "author": {
      "name": "Your Name",
      "email": "you@example.com",
      "date": "2026-01-09T14:12:10Z"
    },
    "committer": {
      "name": "Your Name",
      "email": "you@example.com",
      "date": "2026-01-09T14:12:10Z"
    },
    "message": "ci: empty commit violate ruleset",
    "comment_count": 0,
    "tree": {
      "sha": "70c2323d81843f5ff6d801ede93f200e5a263f11",
      "url": "https://api.github.com/repos/KittyCAD/ruleset-policy-bot/git/trees/70c2323d81843f5ff6d801ede93f200e5a263f11"
    },
    "verification": {
      "verified": false,
      "reason": "unsigned",
      "payload": null,
      "signature": null
    }
  },
  "author": {
    "login": "invalid-email-address",
    "id": 148100,
    "node_id": "MDQ6VXNlcjE0ODEwMA==",
    "avatar_url": "https://avatars.githubusercontent.com/u/148100?v=4",
    "gravatar_id": "",
    "url": "https://api.github.com/users/invalid-email-address",
    "html_url": "https://github.com/invalid-email-address",
    "followers_url": "https://api.github.com/users/invalid-email-address/followers",
    "following_url": "https://api.github.com/users/invalid-email-address/following%7B/other_user%7D",
    "gists_url": "https://api.github.com/users/invalid-email-address/gists%7B/gist_id%7D",
    "starred_url": "https://api.github.com/users/invalid-email-address/starred%7B/owner%7D%7B/repo%7D",
    "subscriptions_url": "https://api.github.com/users/invalid-email-address/subscriptions",
    "organizations_url": "https://api.github.com/users/invalid-email-address/orgs",
    "repos_url": "https://api.github.com/users/invalid-email-address/repos",
    "events_url": "https://api.github.com/users/invalid-email-address/events%7B/privacy%7D",
    "received_events_url": "https://api.github.com/users/invalid-email-address/received_events",
    "type": "User",
    "site_admin": false,
    "name": null,
    "patch_url": null
  },
  "committer": {
    "login": "invalid-email-address",
    "id": 148100,
    "node_id": "MDQ6VXNlcjE0ODEwMA==",
    "avatar_url": "https://avatars.githubusercontent.com/u/148100?v=4",
    "gravatar_id": "",
    "url": "https://api.github.com/users/invalid-email-address",
    "html_url": "https://github.com/invalid-email-address",
    "followers_url": "https://api.github.com/users/invalid-email-address/followers",
    "following_url": "https://api.github.com/users/invalid-email-address/following%7B/other_user%7D",
    "gists_url": "https://api.github.com/users/invalid-email-address/gists%7B/gist_id%7D",
    "starred_url": "https://api.github.com/users/invalid-email-address/starred%7B/owner%7D%7B/repo%7D",
    "subscriptions_url": "https://api.github.com/users/invalid-email-address/subscriptions",
    "organizations_url": "https://api.github.com/users/invalid-email-address/orgs",
    "repos_url": "https://api.github.com/users/invalid-email-address/repos",
    "events_url": "https://api.github.com/users/invalid-email-address/events%7B/privacy%7D",
    "received_events_url": "https://api.github.com/users/invalid-email-address/received_events",
    "type": "User",
    "site_admin": false,
    "name": null,
    "patch_url": null
  },
  "parents": [
    {
      "url": "https://api.github.com/repos/KittyCAD/ruleset-policy-bot/commits/b67a1e80cda53b287d0e01f00a6932d0704c42c2",
      "sha": "b67a1e80cda53b287d0e01f00a6932d0704c42c2",
      "html_url": "https://github.com/KittyCAD/ruleset-policy-bot/commit/b67a1e80cda53b287d0e01f00a6932d0704c42c2"
    }
  ],
  "stats": {
    "total": 0,
    "additions": 0,
    "deletions": 0
  },
  "files": []
}
                "#;

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
    messages: Mutex<RefCell<Vec<(SlackChannelId, SlackMessageContent)>>>,
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
        self.messages
            .lock()
            .as_ref()
            .expect("should not be locked")
            .borrow_mut()
            .push((channel_id, content));
        Ok(())
    }
}

#[tokio::test]
async fn test_updating_rule_suites() {
    let bot = MockRulesetBot {
        events: Mutex::new(RefCell::new(vec![])),
    };
    let slack_client = MockSlackClient {
        messages: Mutex::new(RefCell::new(vec![])),
    };
    process_rule_suites(
        &bot,
        &BotConfig {
            github_org: "KittyCAD".to_string(),
            github_web_base_url: "https://github.com/".to_string(),
            slack_soc2_channel: "#soc2".to_string(),
            review_requirement_ruleset_id: None,
            block_force_push_ruleset_id: None,
            codeowners_ruleset_id: None,
            in_scope_asset_level: AssetLevel::Playground..=AssetLevel::Playground,
            callout_asset_level: AssetLevel::Production..=AssetLevel::Production,
            critical_asset_levels: AssetLevel::Production..=AssetLevel::Production,
            github_auth: GitHubAuth::Token(std::env::var("GH_TOKEN").unwrap()),
        },
        &slack_client,
        "KittyCAD/ruleset-policy-bot",
        "ruleset-policy-bot",
    )
    .await
    .unwrap();

    insta::assert_debug_snapshot!(
        bot.events
            .lock()
            .as_ref()
            .expect("should not be locked")
            .borrow()
            .first()
    );

    insta::assert_debug_snapshot!(
        slack_client
            .messages
            .lock()
            .as_ref()
            .expect("should not be locked")
            .borrow()
            .first()
    );
}

#[tokio::test]
async fn test_evaluate_rule_suites() {
    let rule_suite = RuleSuite {
        id: 1923052992,
        actor_id: Some(905221),
        actor_name: Some("maxammann".to_string()),
        before_sha: "0f61dc3184b58b41465f2cf89c64c22ae626567b".to_string(),
        after_sha: "d6602d2416760fb1bee076fbd895b97e41a0f0f7".to_string(),
        ref_name: "refs/heads/ci-tests".to_string(),
        repository_id: 1108572605,
        repository_name: "ruleset-policy-bot".to_string(),
        pushed_at: DateTime::parse_from_rfc3339("2026-01-09T14:12:10Z")
            .expect("valid datetime")
            .with_timezone(&chrono::Utc),
        result: ruleset_policy_bot::soc2::rule_suit::RuleOutcome::Bypass,
        evaluation_result: None,
        rule_evaluations: Some(vec![
            RuleEvaluation {
                rule_source: RuleSource {
                    typ: "secret_scanning".to_string(),
                    id: None,
                    name: None,
                },
                enforcement: Enforcement::Active,
                result: RuleEvalResult::Pass,
                rule_type: "secret_scanning".to_string(),
                details: None,
            },
            RuleEvaluation {
                rule_source: RuleSource {
                    typ: "ruleset".to_string(),
                    id: Some(11660672),
                    name: Some("Testing".to_string()),
                },
                enforcement: Enforcement::Active,
                result: RuleEvalResult::Fail,
                rule_type: "pull_request".to_string(),
                details: Some("Changes must be made through a pull request.".to_string()),
            },
        ]),
    };

    let bot = MockRulesetBot {
        events: Mutex::new(RefCell::new(vec![NewGithubRuleSuiteEvent {
            github_id: "1923052992".to_string(),
            repository_full_name: "KittyCAD/ruleset-policy-bot".to_string(),
            event_data: serde_json::to_string(&rule_suite).expect("should serialize"),
            resulting_commit: Some(COMMIT.to_string()),
            prs: Some(
                serde_json::to_string::<Vec<PullRequest>>(&vec![]).expect("should serialize"),
            ),
            notified: false,
        }])),
    };

    let slack_client = MockSlackClient {
        messages: Mutex::new(RefCell::new(vec![])),
    };
    let config = BotConfig {
        github_org: "KittyCAD".to_string(),
        github_web_base_url: "https://github.com/".to_string(),
        slack_soc2_channel: "#soc2".to_string(),
        review_requirement_ruleset_id: None,
        block_force_push_ruleset_id: None,
        codeowners_ruleset_id: None,
        in_scope_asset_level: AssetLevel::Playground..=AssetLevel::Playground,
        callout_asset_level: AssetLevel::Production..=AssetLevel::Production,
        critical_asset_levels: AssetLevel::Production..=AssetLevel::Production,
        github_auth: GitHubAuth::Token(std::env::var("GH_TOKEN").unwrap()),
    };
    evaluate_rule_suites(
        &bot,
        &config,
        &slack_client,
        &create_octocrab(&config).expect("should create octocrab"),
        "KittyCAD/ruleset-policy-bot",
        "ruleset-policy-bot",
    )
    .await
    .unwrap();

    let messages = slack_client
        .messages
        .lock()
        .as_ref()
        .expect("should not be locked")
        .borrow()
        .clone();
    assert_eq!(messages.len(), 2); // One to actor one to max
    insta::assert_debug_snapshot!(messages);

    let slack_client = MockSlackClient {
        messages: Mutex::new(RefCell::new(vec![])),
    };

    // Callout

    let config = BotConfig {
        github_org: "KittyCAD".to_string(),
        github_web_base_url: "https://github.com/".to_string(),
        slack_soc2_channel: "#soc2".to_string(),
        review_requirement_ruleset_id: Some(11660672), // pretend the ruleset checks for reviews
        block_force_push_ruleset_id: None,
        codeowners_ruleset_id: None,
        in_scope_asset_level: AssetLevel::Playground..=AssetLevel::Playground,
        callout_asset_level: AssetLevel::Playground..=AssetLevel::Production, // call out anything
        critical_asset_levels: AssetLevel::Playground..=AssetLevel::Production, // everything is critical
        github_auth: GitHubAuth::Token(std::env::var("GH_TOKEN").unwrap()),
    };
    evaluate_rule_suites(
        &bot,
        &config,
        &slack_client,
        &create_octocrab(&config).expect("should create octocrab"),
        "KittyCAD/ruleset-policy-bot",
        "ruleset-policy-bot",
    )
    .await
    .unwrap();

    let messages = slack_client
        .messages
        .lock()
        .as_ref()
        .expect("should not be locked")
        .borrow()
        .clone();
    assert_eq!(messages.len(), 3); // one to max, one to actor, one to soc2 channel
    insta::assert_debug_snapshot!(messages);
}
