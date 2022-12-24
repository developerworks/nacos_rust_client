use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct Env {
    pub profiles: Profiles,
}
impl Display for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_string(&self).unwrap_or_else(|_| {
                let msg = "Failed serialize to json string";
                panic!("{}", msg)
            })
        )
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Profiles {
    pub active: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Application {
    pub shutdown_timeout: u64,
    pub workers: usize,    
    pub hosts: Vec<String>,
    pub hosts_v6: Vec<String>,
    pub port: u16,
    pub log: Log,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ac {
    pub application: Application,
}
#[derive(Serialize, Clone, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct Nacos {
    pub nacos: NacosConfig,
}
#[derive(Serialize, Clone, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct NacosConfig {
    pub server_addr: String,
    pub namespace: String,
    pub group: Option<String>,
    pub data_id: String,
}
#[derive(Serialize, Clone, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct Log {
    pub level: String,
    pub path: String,
    pub rotation: Rotation,
    pub file_name_prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rotation {
    Hourly,
    Daily,
    Never,
}