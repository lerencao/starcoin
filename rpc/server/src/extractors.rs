use crate::metadata::Metadata;


use jsonrpc_http_server::hyper;
use jsonrpc_ipc_server;
use jsonrpc_pubsub::Session;
use jsonrpc_tcp_server;
use jsonrpc_tcp_server::RequestContext;

use std::sync::Arc;

/// Common HTTP & IPC & TCP metadata extractor.
pub struct RpcExtractor;

impl jsonrpc_http_server::MetaExtractor<Metadata> for RpcExtractor {
    fn read_metadata(&self, _req: &hyper::Request<hyper::Body>) -> Metadata {
        Metadata { session: None }
    }
}
impl jsonrpc_ipc_server::MetaExtractor<Metadata> for RpcExtractor {
    fn extract(&self, req: &jsonrpc_ipc_server::RequestContext) -> Metadata {
        Metadata {
            session: Some(Arc::new(Session::new(req.sender.clone()))),
        }
    }
}
impl jsonrpc_tcp_server::MetaExtractor<Metadata> for RpcExtractor {
    fn extract(&self, context: &RequestContext) -> Metadata {
        Metadata {
            session: Some(Arc::new(Session::new(context.sender.clone()))),
        }
    }
}
