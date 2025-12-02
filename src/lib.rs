mod null_date_format;
pub mod soc2;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use slack_morphism::SlackUser;

/// Configuration values that the library consumer must provide
pub trait Config {
    /// Returns the GitHub organization name
    fn github_org(&self) -> &str;

    /// Returns the GitHub web base URL (e.g., "https://github.com")
    fn github_web_base_url(&self) -> &str;

    /// Returns the Slack channel ID for SOC2 events
    fn slack_soc2_channel(&self) -> &str;

    /// Returns the ruleset ID for review requirement checks (optional)
    /// Example: https://github.com/organizations/YOUR_ORG/settings/rules/3973005
    fn review_requirement_ruleset_id(&self) -> Option<i64> {
        None
    }

    /// Returns the ruleset ID for force push blocking (optional)
    /// Example: https://github.com/organizations/YOUR_ORG/settings/rules/5602260
    fn block_force_push_ruleset_id(&self) -> Option<i64> {
        None
    }

    /// Returns the ruleset ID for code owners (optional)
    /// Example: https://github.com/organizations/YOUR_ORG/settings/rules/5619225
    fn codeowners_ruleset_id(&self) -> Option<i64> {
        None
    }
}

/// GitHub App authentication credentials
pub struct GitHubAppCredentials {
    pub app_id: String,
    pub private_key: String,
}

/// GitHub App authentication context
pub struct GitHubAppAuthContext {
    pub credentials: GitHubAppCredentials,
    pub installation_id: i64,
}

/// User information from the database
pub struct User {
    pub email: String,
    pub github_username: Option<String>,
}

/// Slack client abstraction
#[async_trait]
pub trait SlackClient: Send + Sync {
    /// Get a Slack user by their email address
    async fn get_user_by_email(&self, email: &str) -> Result<SlackUserResponse>;

    /// Post a message to a Slack channel or user
    async fn post_message(
        &self,
        request: slack_morphism::api::SlackApiChatPostMessageRequest,
    ) -> Result<()>;
}

/// Response containing a Slack user
pub struct SlackUserResponse {
    pub user: SlackUser,
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

/// Database operations trait that the library consumer must implement
#[async_trait]
pub trait RulesetBot: Send + Sync {
    /// Get GitHub app authentication context
    async fn github_app_auth_context(&self) -> Result<GitHubAppAuthContext>;

    /// Get authenticated Slack client
    async fn get_slack_client(&self) -> Result<Box<dyn SlackClient>>;

    /// Find a GitHub rule suite event by GitHub ID
    async fn find_rule_suite_by_github_id(
        &self,
        github_id: &str,
    ) -> Result<Option<GithubRuleSuiteEvent>>;

    /// Create a new GitHub rule suite event
    async fn create_rule_suite_event(
        &self,
        event: NewGithubRuleSuiteEvent,
    ) -> Result<GithubRuleSuiteEvent>;

    /// Find all unnotified rule suite events for a repository
    async fn find_unnotified_rule_suites(
        &self,
        repository_full_name: &str,
    ) -> Result<Vec<GithubRuleSuiteEvent>>;

    /// Mark a rule suite event as notified
    async fn mark_rule_suite_notified(&self, id: i32) -> Result<()>;

    /// Get a user by GitHub username
    async fn get_user_by_github_username(&self, github_username: &str) -> Result<Option<User>>;

    /// Get configuration
    fn config(&self) -> &dyn Config;
}

pub fn default_date() -> chrono::naive::NaiveDate {
    chrono::naive::NaiveDate::parse_from_str("1970-01-01", "%Y-%m-%d").unwrap()
}
