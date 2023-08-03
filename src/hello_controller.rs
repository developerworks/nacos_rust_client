// use actix_web::dev::HttpServiceFactory;
use actix_web::get;
use actix_web::web;
use actix_web::Responder;
// use opentelemetry::exporter::trace::stdout;
// use opentelemetry::sdk::export::trace::stdout;




#[get("/")]
async fn hello() -> impl Responder {
    "Hello world!"
}

pub fn init(cfg: &mut web::ServiceConfig) {
    cfg.service(hello);
}

// pub fn api_routes() -> impl HttpServiceFactory {
//     web::scope("/patient").service(hello)
// }
