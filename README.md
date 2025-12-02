# Ruleset Policy Bot

This crate provides a trait-based interface for processing GitHub rule suite events and sending Slack notifications for policy violations.

## Overview

The library is designed to be database-agnostic and allows you to provide your own implementations for:
- Database operations (using Diesel, SQLx, or any other backend)
- Slack client configuration
- Application configuration (GitHub org, URLs, channels)

## Core Traits

### 1. `Config` Trait

Provides configuration values for the application.

```rust
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
        None  // Default implementation
    }
    
    /// Returns the ruleset ID for force push blocking (optional)
    /// Example: https://github.com/organizations/YOUR_ORG/settings/rules/5602260
    fn block_force_push_ruleset_id(&self) -> Option<i64> {
        None  // Default implementation
    }
    
    /// Returns the ruleset ID for code owners (optional)
    /// Example: https://github.com/organizations/YOUR_ORG/settings/rules/5619225
    fn codeowners_ruleset_id(&self) -> Option<i64> {
        None  // Default implementation
    }
}
```

**Note:** The ruleset ID methods are optional with default implementations returning `None`. 
Override them if you want to track specific violations for your organization's rulesets.

### 2. `SlackClient` Trait

Abstracts Slack operations.

```rust
#[async_trait]
pub trait SlackClient: Send + Sync {
    async fn get_user_by_email(&self, email: &str) -> Result<SlackUserResponse>;
    async fn post_message(&self, request: SlackApiChatPostMessageRequest) -> Result<()>;
}
```

### 3. `RulesetBot` Trait

The main trait that provides all database operations.

```rust
#[async_trait]
pub trait RulesetBot: Send + Sync {
    // GitHub App authentication
    async fn github_app_auth_context(&self) -> Result<GitHubAppAuthContext>;
    
    // Slack client
    async fn get_slack_client(&self) -> Result<Box<dyn SlackClient>>;
    
    // Rule suite operations
    async fn find_rule_suite_by_github_id(&self, github_id: &str) 
        -> Result<Option<GithubRuleSuiteEvent>>;
    async fn create_rule_suite_event(&self, event: NewGithubRuleSuiteEvent) 
        -> Result<GithubRuleSuiteEvent>;
    async fn find_unnotified_rule_suites(&self, repository_full_name: &str) 
        -> Result<Vec<GithubRuleSuiteEvent>>;
    async fn mark_rule_suite_notified(&self, id: i32) -> Result<()>;
    
    // User operations
    async fn get_user_by_github_username(&self, github_username: &str) 
        -> Result<Option<User>>;
    
    // Configuration
    fn config(&self) -> &dyn Config;
}
```

## Usage Example

### Implementing the Config Trait

```rust
use ruleset_policy_bot::Config;

struct MyConfig {
    github_org: String,
    github_web_base_url: String,
    slack_soc2_channel: String,
}

impl Config for MyConfig {
    fn github_org(&self) -> &str {
        &self.github_org
    }
    
    fn github_web_base_url(&self) -> &str {
        &self.github_web_base_url
    }
    
    fn slack_soc2_channel(&self) -> &str {
        &self.slack_soc2_channel
    }
    
    // Optional: Override to track specific violations
    fn review_requirement_ruleset_id(&self) -> Option<i64> {
        Some(3973005)  // Your organization's review requirement ruleset ID
    }
    
    fn block_force_push_ruleset_id(&self) -> Option<i64> {
        Some(5602260)  // Your organization's force push ruleset ID
    }
    
    // Leave codeowners_ruleset_id as default (None) if you don't want to track it
}
```

### Processing Rule Suites

```rust
use ruleset_policy_bot::RulesetBot;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let bot = MyRulesetBot::new().await?;
    
    // Process rule suites for a repository
    ruleset_policy_bot::soc2::process_rule_suites(
        &bot,
        "my-org/my-repo",
        "my-repo",
    ).await?;
    
    Ok(())
}
```

## Finding Your Ruleset IDs

To find your organization's ruleset IDs:

1. Go to `https://github.com/organizations/YOUR_ORG/settings/rules`
2. Click on a ruleset to view its details
3. The ruleset ID is in the URL: `https://github.com/organizations/YOUR_ORG/settings/rules/RULESET_ID`

### Why Ruleset IDs Matter

The library uses these IDs to identify **critical violations** that should trigger immediate notifications:

- **Review Requirement Ruleset**: Detects when code is merged without proper review
- **Force Push Ruleset**: Detects when someone force-pushes to protected branches
- **Code Owners Ruleset**: Detects when code owners approval is bypassed

If you don't provide these IDs (they default to `None`), the library will still work but won't be able to identify these specific critical violations.

## Data Types

### GithubRuleSuiteEvent

Represents a stored GitHub rule suite event.

```rust
pub struct GithubRuleSuiteEvent {
    pub id: i32,                          // Database record ID
    pub github_id: String,                // GitHub rule suite ID
    pub repository_full_name: String,     // "org/repo"
    pub event_data: String,               // JSON serialized RuleSuite
    pub resulting_commit: Option<String>, // JSON serialized RepoCommit
    pub prs: Option<String>,              // JSON serialized Vec<PullRequest>
    pub notified: bool,                   // Whether notification was sent
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### GitHubAppAuthContext

GitHub App authentication information.

```rust
pub struct GitHubAppAuthContext {
    pub credentials: GitHubAppCredentials,
    pub installation_id: i64,
}

pub struct GitHubAppCredentials {
    pub app_id: String,
    pub private_key: String,  // PEM format
}
```

## Features

The library provides:

- **Rule Suite Processing**: Fetches rule suites from GitHub and stores them
- **Violation Detection**: Identifies policy violations based on asset levels and configured rulesets
- **Slack Notifications**: Sends formatted notifications to Slack channels or DMs
- **Asset Level Support**: Different handling for Production, NonEssentialProduction, etc.
- **Critical Violation Handling**: Special handling for configured critical violations (force push, review requirements, etc.)

