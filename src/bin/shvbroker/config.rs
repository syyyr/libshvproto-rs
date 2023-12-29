use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    #[allow(dead_code)]
    listen: Listen,
    pub users: HashMap<String, User>,
    pub roles: HashMap<String, Role>,
}
#[derive(Serialize, Deserialize, Debug)]
struct Listen {
    #[allow(dead_code)]
    tcp: String,
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
    pub roles: Vec<String>,
    pub access: Vec<AccessRule>,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct AccessRule {
    pub patterns: String,
    pub methods: String,
    pub grant: String,
}
impl AccessRule {}
pub fn default_config() -> Config {
    Config {
        listen: Listen { tcp: "localhost:3755".to_string() },
        users: HashMap::from([
            ("admin".to_string(), User{ password: Password::Plain("admin".into()), roles: vec!["su".to_string()]}),
            ("user".to_string(), User{ password: Password::Plain("user".into()), roles: vec!["client".to_string()]}),
            ("tester".to_string(), User{ password: Password::Sha1("ab4d8d2a5f480a137067da17100271cd176607a1".into()), roles: vec!["tester".to_string()]}),
        ]),
        roles: HashMap::from([
            ("su".to_string(), Role{ roles: vec![], access: vec![
                AccessRule{ patterns: "**".to_string(), methods: "".to_string(), grant: "su".to_string() },
            ] }),
            ("client".to_string(), Role{ roles: vec!["ping".to_string(), "subscribe".to_string(), "browse".to_string(), ], access: vec![] }),
            ("tester".to_string(), Role{ roles: vec!["client".to_string()], access: vec![
                AccessRule{ patterns: "test/**".to_string(), methods: "".to_string(), grant: "cfg".to_string() },
            ] }),
            ("ping".to_string(), Role{ roles: vec![], access: vec![
                AccessRule{ patterns: ".app".to_string(), methods: "ping".to_string(), grant: "wr".to_string() },
            ] }),
            ("subscribe".to_string(), Role{ roles: vec![], access: vec![
                AccessRule{ patterns: ".app/broker/currentClient".to_string(), methods: "subscribe".to_string(), grant: "wr".to_string() },
                AccessRule{ patterns: ".app/broker/currentClient".to_string(), methods: "unsubscribe".to_string(), grant: "wr".to_string() },
            ] }),
            ("browse".to_string(), Role{ roles: vec![], access: vec![
                AccessRule{ patterns: "**".to_string(), methods: "".to_string(), grant: "bws".to_string() },
            ] }),
        ]),
    }
}