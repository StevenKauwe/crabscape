use crate::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub origin: String,
    pub rp: String,
    pub ledger: std::path::PathBuf,
    pub authority: String,
    pub users: HashMap<String, String>,
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        Err("Passkey configuration requires experimental-device-auth build".into())
    }
}
pub fn invalidate_user(_: &mut config::Config, _: &str) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
pub fn sync_users(_: &mut config::Config) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
