// State model — types only, no logic

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Fail,
    Pending,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub required: bool,
    #[serde(default)]
    pub details_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadState {
    Open,
    Addressed,
    Stale,
    Resolved,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Thread {
    pub id: u64,
    pub state: ThreadState,
    pub body: String,
    pub replies: Vec<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PrState {
    pub number: u64,
    pub title: String,
    pub branch: String,
    pub draft: bool,
    pub approved: bool,
    pub checks: Vec<Check>,
    pub threads: Vec<Thread>,
}
