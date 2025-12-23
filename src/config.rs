use crate::utils::env_util::get_env_var;
use anyhow::Error;

#[derive(Clone, Debug)]
pub struct Config {
    pub application: ApplicationSetting,
    pub nexus_setting: NexusSetting,
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct ApplicationSetting {
    pub port: u16,
    pub host: String,
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct NexusSetting {
    pub moka_setting: MokaSetting,
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct MokaSetting {
    pub capacity: u64,
    pub time_to_idle: u64,
}

pub fn get_configuration() -> Result<Config, Error> {
    let moka_setting = MokaSetting {
        capacity: get_env_var("MOKA_CAPACITY", "100000").parse()?,
        time_to_idle: get_env_var("MOKA_TIME_TO_IDLE", "3600").parse()?,
    };

    let config = Config {
        application: ApplicationSetting {
            host: get_env_var("HOST", "localhost"),
            port: get_env_var("PORT", "8888").parse()?,
        },
        nexus_setting: NexusSetting { moka_setting },
    };
    Ok(config)
}
