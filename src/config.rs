

#[derive(Clone)]
pub struct Config {
    pub server: Server
}

#[derive(Clone)]
pub struct Server {
    pub host: String,
    pub port: u16
}
