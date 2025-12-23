use crate::utils::env_util::get_env_var;
use anyhow::Error;
use std::time::Duration;

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
    pub debounce: Duration,
    pub max_debounce: Duration,
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
        nexus_setting: NexusSetting {
            debounce: Duration::from_secs(get_env_var("DEBOUNCE", "3").parse()?),
            max_debounce: Duration::from_secs(get_env_var("MAX_DEBOUNCE", "5").parse()?),
            moka_setting,
        },
    };
    Ok(config)
}
