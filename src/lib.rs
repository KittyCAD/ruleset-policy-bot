mod null_date_format;
pub mod soc2;

use crate::soc2::asset_level::AssetLevel;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use slack_morphism::{SlackChannelId, SlackMessageContent, SlackUser, SlackUserId};
use std::fmt::Debug;
use std::ops::RangeInclusive;

#[derive(Debug, Clone)]
pub struct BotConfig {
    pub github_org: String,
    pub github_web_base_url: String,
    pub slack_soc2_channel: String,
    pub review_requirement_ruleset_id: Option<i64>,
    pub block_force_push_ruleset_id: Option<i64>,
    pub codeowners_ruleset_id: Option<i64>,
    /// The in-scope asset level repos
    pub in_scope_asset_level: RangeInclusive<AssetLevel>,
    /// The range of asset levels that can trigger callouts (there are still exceptions)
    pub callout_asset_level: RangeInclusive<AssetLevel>,
    /// The asset levels that are considered critical
    pub critical_asset_levels: RangeInclusive<AssetLevel>,
    pub github_auth: GitHubAuth,
}

/// GitHub App authentication credentials
#[derive(Clone)]
pub struct GitHubAppCredentials {
    pub app_id: String,
    pub private_key: String,
}

/// GitHub App authentication
#[derive(Clone)]
pub struct GitHubAppInstallation {
    pub credentials: GitHubAppCredentials,
    pub installation_id: i64,
}

/// GitHub authentication
#[derive(Clone)]
pub enum GitHubAuth {
    Installation(GitHubAppInstallation),
    Token(String),
}

impl Debug for GitHubAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GitHubAuth { ... }")
    }
}

/// User information from the database
pub struct User {
    pub email: String,
}

/// Slack client abstraction
#[async_trait]
pub trait SlackClient: Send + Sync {
    /// Get a Slack user by their email address
    async fn get_user_by_email(&self, email: &str) -> Result<SlackUser>;

    /// Post a message to a Slack channel
    async fn post_message_channel(
        &self,
        channel_id: SlackChannelId,
        content: SlackMessageContent,
    ) -> Result<()>;

    /// Post a message to a Slack user
    async fn post_message_user(
        &self,
        user_id: SlackUserId,
        content: SlackMessageContent,
    ) -> Result<()>;
}

/// Database operations trait that the library consumer must implement
#[async_trait]
pub trait RulesetBot: Send + Sync {
    /// Find a GitHub rule suite event by GitHub ID
    async fn find_rule_suite_by_github_id(
        &self,
        github_id: &str,
    ) -> Result<Option<GithubRuleSuiteEvent>>;

    /// Create a new GitHub rule suite event
    async fn create_rule_suite_event(&self, event: NewGithubRuleSuiteEvent) -> Result<()>;

    /// Find all unnotified rule suite events for a repository
    async fn find_unnotified_rule_suites(
        &self,
        repository_full_name: &str,
    ) -> Result<Vec<GithubRuleSuiteEvent>>;

    /// Mark a rule suite event as notified
    async fn mark_rule_suite_notified(&self, id: i32) -> Result<()>;

    /// Get a user by GitHub username
    async fn get_email_by_github_username(&self, github_username: &str) -> Result<Option<String>>;
}

/// GitHub rule suite event storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubRuleSuiteEvent {
    /// The rule suite id. Actually the one from GitHub
    pub id: i32,
    pub github_id: String,
    pub repository_full_name: String,
    /// JSON serialized [`RuleSuite`]
    pub event_data: String,
    /// JSON serialized [`RepoCommit`]
    pub resulting_commit: Option<String>,
    /// JSON serialized array of [`PullRequest`]
    pub prs: Option<String>,
    /// Whether a notification has been sent for this record (e.g. to Slack).
    pub notified: bool,
    /// When the record was created.
    #[serde(deserialize_with = "crate::null_date_format::deserialize")]
    pub created_at: DateTime<Utc>,
    /// When the record was last updated.
    #[serde(deserialize_with = "crate::null_date_format::deserialize")]
    pub updated_at: DateTime<Utc>,
}

/// New GitHub rule suite event to be created
#[derive(Debug, Clone)]
pub struct NewGithubRuleSuiteEvent {
    pub github_id: String,
    pub repository_full_name: String,
    pub event_data: String,
    pub resulting_commit: Option<String>,
    pub prs: Option<String>,
    pub notified: bool,
}

pub fn default_date() -> chrono::naive::NaiveDate {
    chrono::naive::NaiveDate::parse_from_str("1970-01-01", "%Y-%m-%d").unwrap()
}
