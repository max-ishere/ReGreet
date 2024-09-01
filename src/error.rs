use thiserror::Error;

#[derive(Error, Debug)]
pub enum TomlReadError {
    #[error("I/O error: {0}")]
    IO(#[from] std::io::Error),
    #[error("TOML parsing error: {0}")]
    Deserialize(#[from] toml::de::Error),
}

#[derive(Error, Debug)]
pub enum TomlWriteError {
    #[error("I/O error: {0}")]
    IO(#[from] std::io::Error),
    #[error("TOML generation error: {0}")]
    Serialize(#[from] toml::ser::Error),
}
