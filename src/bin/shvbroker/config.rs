use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub listen: Listen,
    pub editable_access: bool,
    pub data_directory: Option<String>,
    pub users: HashMap<String, User>,
    pub roles: HashMap<String, Role>,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Listen {
    pub tcp: Option<String>,
    pub ssl: Option<String>,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    pub password: Password,
    pub roles: Vec<String>,
}
#[derive(Serialize, Deserialize, Debug)]
pub enum Password {
    Plain(String),
    Sha1(String),
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Role {
    pub roles: Option<Vec<String>>,
    pub access: Option<Vec<AccessRule>>,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct AccessRule {
    pub paths: String,
    pub methods: String,
    pub grant: String,
}
impl AccessRule {}
pub fn default_config() -> Config {
    Config {
        listen: Listen { tcp: Some("localhost:3755".to_string()), ssl: None },
        editable_access: false,
        data_directory: None,
        users: HashMap::from([
            ("admin".to_string(), User{ password: Password::Plain("admin".into()), roles: vec!["su".to_string()]}),
            ("user".to_string(), User{ password: Password::Plain("user".into()), roles: vec!["client".to_string()]}),
            ("tester".to_string(), User{ password: Password::Sha1("ab4d8d2a5f480a137067da17100271cd176607a1".into()), roles: vec!["tester".to_string()]}),
        ]),
        roles: HashMap::from([
            ("su".to_string(), Role{ roles: None, access: vec![
                AccessRule{ paths: "**".to_string(), methods: "".to_string(), grant: "su".to_string() },
            ].into() }),
            ("client".to_string(), Role{ roles: Some(vec!["ping".to_string(), "subscribe".to_string(), "browse".to_string()]), access: None }),
            ("tester".to_string(), Role{ roles: vec!["client".to_string()].into(), access: vec![
                AccessRule{ paths: "test/**".to_string(), methods: "".to_string(), grant: "cfg".to_string() },
            ].into() }),
            ("ping".to_string(), Role{ roles: None, access: vec![
                AccessRule{ paths: ".app".to_string(), methods: "ping".to_string(), grant: "wr".to_string() },
            ].into() }),
            ("subscribe".to_string(), Role{ roles: None, access: vec![
                AccessRule{ paths: ".app/broker/currentClient".to_string(), methods: "subscribe".to_string(), grant: "wr".to_string() },
                AccessRule{ paths: ".app/broker/currentClient".to_string(), methods: "unsubscribe".to_string(), grant: "wr".to_string() },
            ].into() }),
            ("browse".to_string(), Role{ roles: None, access: vec![
                AccessRule{ paths: "**".to_string(), methods: "".to_string(), grant: "bws".to_string() },
            ].into() }),
        ]),
    }
}