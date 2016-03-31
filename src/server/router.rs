//! A dead-simple router implementation
//!
//! A `Router` simply matches a request-uri against installed routes, in the
//! order they have been added, dispatching to the first handler that matches.

use errors::*;
use server::{Handler, Request, Response, Fresh};
use server::error_messages::*;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Router {
    routes: Vec<Route>
}

struct Route {
    path: PathBuf,
    handlers: MethodDispatch
}

enum MethodDispatch {
    Any(Box<Handler>),
    Specific(HashMap<String, Box<Handler>>)
}

impl Router {
    fn serve_inner(&self, req: Request, res: Response<Fresh>) -> Result<()> {
        let request_path = Path::new(req.request_uri()).to_owned();

        for route in &self.routes {
            if request_path.starts_with(&route.path) {
                route.handlers.serve(req, res);
                return Ok(());
            }
        }

        try!(error_404(res));
        Ok(())
    }

    /// Initialize a new, empty router
    pub fn new() -> Router {
        Router { routes: Vec::new() }
    }

    /// Create a route that will invoke the given `handler` for all methods
    pub fn route_any<H: Handler + 'static>(&mut self, path: PathBuf, handler: H)
    {
        self.routes.push(Route {
            path: path,
            handlers: MethodDispatch::Any(Box::new(handler))
        });
    }

    /// Create a route that will invoke the given `handler`, but only for the
    /// particular `method`.
    pub fn route<H: Handler + 'static>(&mut self, path: PathBuf, method: String,
                                       handler: H) {
        for route in self.routes.iter_mut() {
            if route.path == path {
                match &mut route.handlers {
                    &mut MethodDispatch::Specific(ref mut map) =>
                    {map.insert(method, Box::new(handler));},
                    &mut MethodDispatch::Any(_) =>
                    {panic!("Tried to add a universal and method-specific route for the same prefix");}
                }
                return;
            }
        }

        let mut handlers: HashMap<_, Box<Handler>> = HashMap::new();
        handlers.insert(method, Box::new(handler));

        self.routes.push(Route {
            path: path,
            handlers: MethodDispatch::Specific(handlers)
        });
    }
}

impl Handler for Router {
    fn serve(&self, req: Request, res: Response<Fresh>) {
        match self.serve_inner(req, res) {
            Ok(_) => (),
            Err(e) => warn!("Error serving a request: {:?}", e)
        }
    }
}

impl Handler for MethodDispatch {
    fn serve(&self, req: Request, res: Response<Fresh>) {
        match self {
            &MethodDispatch::Any(ref handler) => handler.serve(req, res),
            &MethodDispatch::Specific(ref map) => {
                if let Some(handler) = map.get(req.method()) {
                    handler.serve(req, res);
                }
                else {
                    let _ = error_405(res);
                }
            }
        }
    }
}
