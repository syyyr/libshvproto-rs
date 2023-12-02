use std::collections::HashMap;

pub struct Config {
    listen: Listen,
    users: HashMap<String, User>,
    roles: HashMap<String, Role>,
}
struct Listen {
    tcp: String,
}
struct User {
    password: Password,
    roles: Vec<String>,
}
enum Password {
    Plain(String),
    Sha1(String),
}
struct Role {
    roles: Vec<String>,
    access: Vec<AccessRule>,
}
struct AccessRule {
    path: String,
    method: String,
    grant: String,
}

pub fn default_config() -> Config {
    Config {
        listen: Listen { tcp: "localhost:3755".to_string() },
        users: HashMap::from([
            ("admin".to_string(), User{ password: Password::Plain("admin".into()), roles: vec!["su".to_string()]}),
            ("user".to_string(), User{ password: Password::Plain("user".into()), roles: vec!["client".to_string()]}),
            ("tester".to_string(), User{ password: Password::Plain("service".into()), roles: vec!["tester".to_string()]}),
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