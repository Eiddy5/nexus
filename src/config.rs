use crate::utils::env_util::get_env_var;
use anyhow::Error;

#[derive(Clone, Debug)]
pub struct Config {
    pub application: ApplicationSetting,
}


#[derive(serde::Deserialize, Clone, Debug)]
pub struct ApplicationSetting {
    pub port: u16,
    pub host: String,
}

pub fn get_configuration() -> Result<Config, Error> {
    let config = Config {
        application: ApplicationSetting {
            host: get_env_var("HOST", "localhost"),
            port: get_env_var("PORT", "8888").parse()?,
        },
    };
    Ok(config)
}
