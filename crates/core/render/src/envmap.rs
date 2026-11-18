#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvMap {
    CheapEnvironment,
    Environment,
    WsClassicEnvironment,
    WsEnvironment,
}

impl EnvMap {
    #[must_use]
    pub fn index(self) -> u32 {
        match self {
            Self::CheapEnvironment => 1,
            Self::Environment => 2,
            Self::WsClassicEnvironment => 3,
            Self::WsEnvironment => 4,
        }
    }
}
