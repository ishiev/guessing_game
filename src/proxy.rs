use warp::{
    http::Response,
    Rejection, 
    hyper::body::Bytes,
    hyper::HeaderMap,
    filters::path::FullPath
};

use warp_reverse_proxy::{
    proxy_to_and_forward_response,
    QueryParameters,
    Method,
};

use log::{info, error};

use crate::datacache::{DataCache, rq_hash_string};


pub trait ProxyConfig {
    fn get_proxy_address(&self) -> String;
    fn get_host(&self) -> String { 
        String::default() 
    }
    fn get_base_path(&self) -> String { 
        String::default() 
    }
}

#[derive(Clone)]
pub struct CacheProxy {
    cache: DataCache,
    proxy_address: String,
    host: String,
    base_path: String,
}

impl CacheProxy {
    pub fn new<T: ProxyConfig>(cache: DataCache, config: &T) -> Self {
        CacheProxy {
            cache,
            proxy_address: config.get_proxy_address(),
            host: config.get_host(),
            base_path: config.get_base_path()
        }
    }

    pub async fn handler(
        self: Box<Self>,
        uri: FullPath,
        params: QueryParameters,
        method: Method,
        mut headers: HeaderMap,
        body: Bytes,
    ) -> Result<Response<Bytes>, Rejection> {
        let hash = rq_hash_string(&uri, &body);
        info!(
            "[{}] request hash",
            &hash[..6]
        );

        if let Ok(Some(bytes)) = self.cache.get(&hash) {
            // ignore errors
            info!(
                "[{}] return cached response",
                &hash[..6]
            );
            Ok(Response::new(bytes))
        } else {
            // insert host header from config
            headers.insert("host", self.host.parse().unwrap());
            // proxy to destination and return response
            match proxy_to_and_forward_response(
                self.proxy_address,
                self.base_path,
                uri,
                params,
                method,
                headers,
                body
            ).await {
                Ok(res) => {
                    // save body to cache
                    if let Err(e) = self.cache.insert(&hash, res.body()) {
                        error!(
                            "[{}] error saving response to datacashe, {}",
                            &hash[..6], e
                        )
                    } else {
                        info!("[{}] new response saved to cache",
                        &hash[..6],)
                    }
                    Ok(res)
                }
                Err(err) => Err(err)
            }
        }
    }
}
