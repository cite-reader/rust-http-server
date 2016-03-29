pub mod parser;

use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::path::PathBuf;

/// A holder for app configuration
#[derive(Debug)]
pub struct Config {
    /// Port number to listen on
    pub port: u16,
    pub stat: StaticFilesConfig,
    pub fcgi: FastCgiConfig
}

impl Default for Config {
    fn default() -> Config {
        Config {
            port: 8000,
            stat: Default::default(),
            fcgi: Default::default()
        }
    }
}

#[derive(Debug)]
pub struct StaticFilesConfig {
    /// Where the files are located on disk
    pub webroot: PathBuf,
    /// Public URI prefix that gets mapped onto `webroot`
    pub public_prefix: PathBuf
}

impl Default for StaticFilesConfig {
    fn default() -> StaticFilesConfig {
        StaticFilesConfig {
            webroot: PathBuf::from("/etc/http-server/site"),
            public_prefix: PathBuf::from("/html")
        }
    }
}

#[derive(Debug)]
pub struct FastCgiConfig {
    /// Socket addresses suitable for passing to `TcpStream::connect`.
    pub address: SocketAddr
}

impl Default for FastCgiConfig {
    fn default() -> FastCgiConfig {
        FastCgiConfig {
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                                     9000)
        }
    }
}

