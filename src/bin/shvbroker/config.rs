use std::collections::HashMap;
use glob::Pattern;

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
    pub path: Pattern,
    pub method: Pattern,
    pub grant: String,
}
impl AccessRule {
    pub fn new(path: &str, method: &str, grant: &str) -> shv::Result<Self> {
        let method = if method.is_empty() { "?*" } else { method };
        let path = if path.is_empty() { "**" } else { path };
        match Pattern::new(method) {
            Ok(method) => {
                match Pattern::new(path) {
                    Ok(path) => {
                        Ok(Self {
                            method,
                            path,
                            grant: grant.to_string(),
                        })
                    }
                    Err(err) => { Err(format!("{}", &err).into()) }
                }
            }
            Err(err) => { Err(format!("{}", &err).into()) }
        }
    }
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
                AccessRule::new("**", "", "su").unwrap(),
            ] }),
            ("client".to_string(), Role{ roles: vec!["ping".to_string(), "subscribe".to_string(), "browse".to_string(), ], access: vec![] }),
            ("tester".to_string(), Role{ roles: vec!["client".to_string()], access: vec![
                AccessRule::new("test/**", "", "cfg").unwrap(),
            ] }),
            ("ping".to_string(), Role{ roles: vec![], access: vec![
                AccessRule::new(".app", "ping", "wr").unwrap(),
            ] }),
            ("subscribe".to_string(), Role{ roles: vec![], access: vec![
                AccessRule::new(".app/broker/currentClient", "subscribe", "wr").unwrap(),
                AccessRule::new(".app/broker/currentClient", "unsubscribe", "wr").unwrap(),
            ] }),
            ("browse".to_string(), Role{ roles: vec![], access: vec![
                AccessRule::new("**", "", "bws").unwrap(),
            ] }),
        ]),
    }
}