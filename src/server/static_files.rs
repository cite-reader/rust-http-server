//! Handlers for static file service

use super::{Handler, Request, Response, Fresh, mime_as_string};
use super::error_messages::*;
use config::Config;
use errors::*;

use mime_guess::guess_mime_type_opt;

use std::ffi::OsStr;
use std::fs::{File, canonicalize};
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt;

/// A handler for static files
pub struct Statics {
    conf: Config
}

impl Statics {
    pub fn new(conf: Config) -> Statics {
        Statics { conf: conf }
    }

    fn serve_file(&self, req: Request, mut res: Response<Fresh>) -> Result<()> {
        let request_uri_relative = OsStr::from_bytes(
            &req.request_uri().as_bytes()[1..]
        );

        let requested_file =
            match canonicalize(self.conf.stat.webroot
                               .join(request_uri_relative)) {
                Ok(f) => f,
                Err(e) => {
                    match e.kind() {
                        ErrorKind::NotFound => try!(error_404(res)),
                        _ => try!(error_500(res))
                    };

                    return Err(Error::from(e));
                }
            };

        if !requested_file.starts_with(&self.conf.stat.webroot) {
            let _ = error_403(res);
            return Err(Error::PermissionDenied);
        }

        let file = match File::open(&requested_file) {
            Ok(f) => f,
            Err(e) => {
                match e.kind() {
                    ErrorKind::NotFound => try!(error_404(res)),
                    _ => try!(error_500(res))
                };

                return Err(Error::from(e));
            }
        };

        let meta = match file.metadata() {
            Ok(m) => m,
            Err(e) => {
                try!(error_500(res));
                return Err(Error::from(e));
            }
        };

        if meta.is_dir() {
            try!(error_403(res));
            return Err(Error::PermissionDenied);
        }

        let mime = guess_mime_type_opt(&requested_file)
            .map(mime_as_string)
            .unwrap_or(String::from("application/octet-stream"));

        res.headers_mut().insert("Content-type", mime.into_bytes());
        res.headers_mut().insert("Content-length",
                                 format!("{}", meta.len()).into_bytes());

        Ok(try!(res.of_stream(file)))
    }
}

impl Handler for Statics {
    fn serve(&self, req: Request, res: Response<Fresh>) {
        match self.serve_file(req, res) {
            Ok(_) => (),
            Err(e) => warn!("Error serving a file: {:?}", e)
        }
    }
}
