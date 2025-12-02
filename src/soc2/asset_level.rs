use octocrab::Octocrab;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CustomProperty {
    pub property_name: String,
    pub value: Option<CustomPropertyValue>,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum CustomPropertyValue {
    String(String),
    Array(Vec<String>),
}

pub trait CustomPropertyExt {
    fn list_custom_properties(
        &self,
        owner: &str,
        repo: &str,
    ) -> impl std::future::Future<Output = octocrab::Result<Vec<CustomProperty>>> + Send;
}

impl CustomPropertyExt for Octocrab {
    async fn list_custom_properties(
        &self,
        owner: &str,
        repo: &str,
    ) -> anyhow::Result<Vec<CustomProperty>, octocrab::Error> {
        self.get(
            format!("/repos/{owner}/{repo}/properties/values"),
            None::<&()>,
        )
        .await
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum AssetLevel {
    Production,
    /// Just testing the waters. Not even development breaks if this breaks.
    Playground,
    /// Used for development. If this is pwned, other security controls should stop spreading to production.
    #[serde(rename = "Research & Development")]
    ResearchNDevelopment,
    /// Only relevant for internal folks. No link to production.
    Corporate,
    /// Publicly accessible services, but not part of our core product like store.zoo.dev.
    #[serde(rename = "Non-essential Production")]
    NonEssentialProduction,
}

impl AssetLevel {
    pub fn get_from_props(props: &[CustomProperty]) -> Option<AssetLevel> {
        props
            .iter()
            .find(|prop| prop.property_name == "repository-level")
            .and_then(|prop| match &prop.value {
                None => None,
                Some(CustomPropertyValue::Array(_array)) => {
                    panic!("Array not supported for repository-level")
                }
                Some(CustomPropertyValue::String(str)) => match str.as_str() {
                    "Production" => Some(AssetLevel::Production),
                    "Playground" => Some(AssetLevel::Playground),
                    "Research & Development" => Some(AssetLevel::ResearchNDevelopment),
                    "Corporate" => Some(AssetLevel::Corporate),
                    "Non-essential Production" => Some(AssetLevel::NonEssentialProduction),
                    _ => None,
                },
            })
    }
}
