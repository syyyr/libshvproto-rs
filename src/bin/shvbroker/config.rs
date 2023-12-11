use std::collections::HashMap;

pub struct Config {
    #[allow(dead_code)]
    listen: Listen,
    pub users: HashMap<String, User>,
    pub roles: HashMap<String, Role>,
}
struct Listen {
    #[allow(dead_code)]
    tcp: String,
}
pub struct User {
    pub password: Password,
    pub roles: Vec<String>,
}
pub enum Password {
    Plain(String),
    Sha1(String),
}
pub struct Role {
    pub roles: Vec<String>,
    pub access: Vec<AccessRule>,
}
pub struct AccessRule {
    pub path: String,
    pub method: String,
    pub grant: String,
}

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
                AccessRule{ path: "**".to_string(), method: "".to_string(), grant: "su".to_string() },
            ] }),
            ("client".to_string(), Role{ roles: vec!["ping".to_string(), "subscribe".to_string(), "browse".to_string(), ], access: vec![] }),
            ("tester".to_string(), Role{ roles: vec!["client".to_string()], access: vec![
                AccessRule{ path: "test/**".to_string(), method: "".to_string(), grant: "cfg".to_string() },
            ] }),
            ("ping".to_string(), Role{ roles: vec![], access: vec![
                AccessRule{ path: ".app".to_string(), method: "ping".to_string(), grant: "wr".to_string() },
            ] }),
            ("subscribe".to_string(), Role{ roles: vec![], access: vec![
                AccessRule{ path: ".app/broker/currentClient".to_string(), method: "subscribe".to_string(), grant: "wr".to_string()},
                AccessRule{ path: ".app/broker/currentClient".to_string(), method: "unsubscribe".to_string(), grant: "wr".to_string()},
            ] }),
            ("browse".to_string(), Role{ roles: vec![], access: vec![
                AccessRule{ path: "**".to_string(), method: "".to_string(), grant: "bws".to_string() },
            ] }),
        ]),
    }
}