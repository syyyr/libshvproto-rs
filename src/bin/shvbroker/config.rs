use std::collections::HashMap;
use std::fs;
use serde::{Serialize, Deserialize};
#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub listen: Listen,
    #[serde(default)]
    pub editable_access: bool,
    #[serde(default)]
    pub data_directory: Option<String>,
    #[serde(default)]
    pub access: AccessControl,
}
impl Config {
    pub fn from_file(file_name: &str) -> shv::Result<Self> {
        let content = fs::read_to_string(file_name)?;
        Ok(serde_yaml::from_str(&content)?)
    }
}
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct AccessControl {
    pub users: HashMap<String, User>,
    pub roles: HashMap<String, Role>,
}
impl AccessControl {
    pub fn from_file(file_name: &str) -> shv::Result<Self> {
        let content = fs::read_to_string(file_name)?;
        Ok(serde_yaml::from_str(&content)?)
    }
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Listen {
    #[serde(default)]
    pub tcp: Option<String>,
    #[serde(default)]
    pub ssl: Option<String>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    pub password: Password,
    pub roles: Vec<String>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Password {
    Plain(String),
    Sha1(String),
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Role {
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub access: Vec<AccessRule>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AccessRule {
    pub paths: String,
    #[serde(default)]
    pub methods: String,
    pub grant: String,
}

pub fn default_config() -> Config {
    Config {
        listen: Listen { tcp: Some("localhost:3755".to_string()), ssl: None },
        editable_access: false,
        data_directory: None,
        access: AccessControl {
            users: HashMap::from([
                ("admin".to_string(), User { password: Password::Plain("admin".into()), roles: vec!["su".to_string()] }),
                ("user".to_string(), User { password: Password::Plain("user".into()), roles: vec!["client".to_string()] }),
                ("tester".to_string(), User { password: Password::Sha1("ab4d8d2a5f480a137067da17100271cd176607a1".into()), roles: vec!["tester".to_string()] }),
            ]),
            roles: HashMap::from([
                ("su".to_string(), Role {
                    roles: vec![],
                    access: vec![
                        AccessRule { paths: "**".to_string(), methods: "".to_string(), grant: "su".to_string() },
                    ].into(),
                }),
                ("client".to_string(), Role { roles: vec!["ping".to_string(), "subscribe".to_string(), "browse".to_string()], access: vec![] }),
                ("tester".to_string(), Role {
                    roles: vec!["client".to_string()].into(),
                    access: vec![
                        AccessRule { paths: "test/**".to_string(), methods: "".to_string(), grant: "cfg".to_string() },
                    ],
                }),
                ("ping".to_string(), Role {
                    roles: vec![],
                    access: vec![
                        AccessRule { paths: ".app".to_string(), methods: "ping".to_string(), grant: "wr".to_string() },
                    ],
                }),
                ("subscribe".to_string(), Role {
                    roles: vec![],
                    access: vec![
                        AccessRule { paths: ".app/broker/currentClient".to_string(), methods: "subscribe".to_string(), grant: "wr".to_string() },
                        AccessRule { paths: ".app/broker/currentClient".to_string(), methods: "unsubscribe".to_string(), grant: "wr".to_string() },
                    ],
                }),
                ("browse".to_string(), Role {
                    roles: vec![],
                    access: vec![
                        AccessRule { paths: "**".to_string(), methods: "".to_string(), grant: "bws".to_string() },
                    ],
                }),
            ]),
        },
    }
}