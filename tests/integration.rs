use async_trait::async_trait;
use chrono::DateTime;
use octocrab::models::pulls::PullRequest;
use octocrab::models::repos::RepoCommit;
use ruleset_policy_bot::soc2::asset_level::AssetLevel;
use ruleset_policy_bot::soc2::rule_suit::{
    Enforcement, RuleEvalResult, RuleEvaluation, RuleSource, RuleSuite,
};
use ruleset_policy_bot::soc2::{
    create_octocrab, evaluate_rule_suites, process_rule_suites, send_violation_slack_message,
};
use ruleset_policy_bot::{
    BotConfig, GitHubAuth, GithubRuleSuiteEvent, NewGithubRuleSuiteEvent, RulesetBot, SlackClient,
};
use slack_morphism::{SlackChannelId, SlackMessageContent, SlackUser, SlackUserFlags, SlackUserId};
use std::cell::RefCell;
use std::sync::Mutex;

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
        _github_id: &str,
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
        _github_username: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(Some("max.ammann@zoo.dev".to_string()))
    }
}

struct MockSlackClient {
    messages: Mutex<RefCell<Vec<(String, SlackMessageContent)>>>,
}

#[async_trait]
impl SlackClient for MockSlackClient {
    async fn get_user_by_email(&self, email: &str) -> anyhow::Result<SlackUser> {
        Ok(SlackUser::new(
            SlackUserId(email.to_string()),
            SlackUserFlags::new(),
        ))
    }

    async fn post_message_channel(
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
            .push((channel_id.0, content));
        Ok(())
    }

    async fn post_message_user(
        &self,
        user_id: SlackUserId,
        content: SlackMessageContent,
    ) -> anyhow::Result<()> {
        println!("Posted message to user {}", user_id);
        self.messages
            .lock()
            .as_ref()
            .expect("should not be locked")
            .borrow_mut()
            .push((user_id.0, content));
        Ok(())
    }
}

fn synthetic_merge_commit() -> RepoCommit {
    let mut commit: serde_json::Value =
        serde_json::from_str(COMMIT).expect("commit fixture should be valid JSON");
    let base_sha = "9cf761cb174bc6a7e7179a659ff4a67ee4ac2bb4";
    let head_sha = "4d840b7d08941ac27014dee0c2e96bc5aad26d31";
    commit["sha"] = "a1267306b44ab57752efe8b253a00ad149188560".into();
    commit["commit"]["message"] = format!("Merge {head_sha} into {base_sha}").into();
    commit["committer"]["login"] = "web-flow".into();
    commit["parents"] = serde_json::json!([
        {
            "url": format!("https://api.github.com/repos/KittyCAD/modeling-app/commits/{base_sha}"),
            "sha": base_sha,
            "html_url": format!("https://github.com/KittyCAD/modeling-app/commit/{base_sha}")
        },
        {
            "url": format!("https://api.github.com/repos/KittyCAD/modeling-app/commits/{head_sha}"),
            "sha": head_sha,
            "html_url": format!("https://github.com/KittyCAD/modeling-app/commit/{head_sha}")
        }
    ]);
    serde_json::from_value(commit).expect("synthetic merge commit fixture should deserialize")
}

fn linear_history_rule_suite() -> RuleSuite {
    RuleSuite {
        id: 1923052992,
        actor_id: Some(905221),
        actor_name: Some("adamchalmers".to_string()),
        before_sha: "9cf761cb174bc6a7e7179a659ff4a67ee4ac2bb4".to_string(),
        after_sha: "a1267306b44ab57752efe8b253a00ad149188560".to_string(),
        ref_name: "refs/heads/main".to_string(),
        repository_id: 1108572605,
        repository_name: "modeling-app".to_string(),
        pushed_at: DateTime::parse_from_rfc3339("2026-08-13T22:03:32Z")
            .expect("valid datetime")
            .with_timezone(&chrono::Utc),
        result: ruleset_policy_bot::soc2::rule_suit::RuleOutcome::Bypass,
        evaluation_result: None,
        rule_evaluations: Some(vec![
            failed_evaluation("required_linear_history"),
            RuleEvaluation {
                rule_source: RuleSource {
                    typ: "ruleset".to_string(),
                    id: Some(9777775),
                    name: Some("Test ruleset".to_string()),
                },
                enforcement: Enforcement::Active,
                result: RuleEvalResult::Pass,
                rule_type: "creation".to_string(),
                details: None,
            },
        ]),
    }
}

fn failed_evaluation(rule_type: &str) -> RuleEvaluation {
    RuleEvaluation {
        rule_source: RuleSource {
            typ: "ruleset".to_string(),
            id: Some(9777775),
            name: Some("Test ruleset".to_string()),
        },
        enforcement: Enforcement::Active,
        result: RuleEvalResult::Fail,
        rule_type: rule_type.to_string(),
        details: None,
    }
}

fn test_config() -> BotConfig {
    BotConfig {
        github_org: "KittyCAD".to_string(),
        github_web_base_url: "https://github.com".to_string(),
        slack_soc2_channel: "#soc2".to_string(),
        review_requirement_ruleset_id: None,
        block_force_push_ruleset_id: None,
        codeowners_ruleset_id: None,
        in_scope_asset_level: AssetLevel::Playground..=AssetLevel::Production,
        callout_asset_level: AssetLevel::Production..=AssetLevel::Production,
        critical_asset_levels: AssetLevel::Production..=AssetLevel::Production,
        github_auth: GitHubAuth::Token(String::new()),
    }
}

async fn violation_messages(
    suite: &RuleSuite,
    commit: RepoCommit,
) -> Vec<(String, SlackMessageContent)> {
    let bot = MockRulesetBot {
        events: Mutex::new(RefCell::new(vec![])),
    };
    let slack = MockSlackClient {
        messages: Mutex::new(RefCell::new(vec![])),
    };

    send_violation_slack_message(
        &slack,
        suite,
        Some(commit),
        None,
        AssetLevel::Production,
        &bot,
        &test_config(),
    )
    .await
    .expect("violation should be handled");

    slack
        .messages
        .into_inner()
        .expect("messages should not be locked")
        .into_inner()
}

#[tokio::test]
async fn ignores_linear_history_failure_for_github_test_merge_commit() {
    let messages = violation_messages(&linear_history_rule_suite(), synthetic_merge_commit()).await;

    assert!(
        messages.is_empty(),
        "GitHub's prospective merge commit is not a policy violation"
    );
}

#[tokio::test]
async fn keeps_other_failures_from_github_test_merge_commit() {
    let mut suite = linear_history_rule_suite();
    suite
        .rule_evaluations
        .as_mut()
        .expect("suite should have evaluations")
        .push(failed_evaluation("pull_request"));

    let messages = violation_messages(&suite, synthetic_merge_commit()).await;
    assert_eq!(
        messages.len(),
        2,
        "the actor and SOC2 owner should be notified"
    );
    for (_, message) in messages {
        let fallback = message
            .text
            .expect("notification should have fallback text");
        assert!(fallback.contains("pull_request"));
        assert!(!fallback.contains("required_linear_history"));
    }
}

#[tokio::test]
async fn keeps_linear_history_failure_for_github_ui_merge_commit() {
    let mut commit = synthetic_merge_commit();
    commit.commit.message = "Merge pull request #12976 from KittyCAD/achalmers/kcl-v3".to_string();

    let messages = violation_messages(&linear_history_rule_suite(), commit).await;
    assert_eq!(
        messages.len(),
        2,
        "the actor and SOC2 owner should be notified"
    );
    assert!(messages.iter().all(|(_, message)| {
        message
            .text
            .as_ref()
            .is_some_and(|text| text.contains("required_linear_history"))
    }));
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

    let events = bot.events.lock();
    let events = events.as_ref().expect("should not be locked").borrow();
    let event = events.first().expect("should have event");

    assert!(!event.event_data.is_empty());

    let message = slack_client.messages.lock();
    let message = message.as_ref().expect("should not be locked").borrow();
    let message = message.first().expect("should have message");
    insta::assert_debug_snapshot!(message.1.blocks);
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
