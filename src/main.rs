use actix_web::{App, HttpServer};
use error_chain::error_chain;
use futures::future;
use nacos_rust_client::types::{Ac, Rotation};
use nacos_sdk::api::{
    config::{ConfigChangeListener, ConfigResponse},
    constants,
    naming::{NamingChangeEvent, NamingEventListener},
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    filter, fmt,
    prelude::__tracing_subscriber_SubscriberExt,
    reload::{self, Handle},
    util::SubscriberInitExt,
    Registry,
};
// static mut reload_handle: Option<Handle<LevelFilter, Registry>> = None;

error_chain! {
    foreign_links {
        Io(std::io::Error);
        NacosError(nacos_sdk::api::error::Error);
    }
}

#[actix_web::main]
async fn main() -> Result<()> {
    let env = &nacos_rust_client::get_application_profile();
    let mut conf = nacos_rust_client::get_application_profile_active(&env.profiles.active);
    conf.nacos.data_id = conf.nacos.data_id[..].replace("{}", &env.profiles.active);

    // -----------------------------
    // Get configurations from nacos
    // -----------------------------

    let (ac, mut cs) = nacos_rust_client::get_config().unwrap();
    tracing::debug!("application configurations: {:?}", ac.clone());

    // let reload_handle = nacos_rust_client::init_tracing(&ac.application.log);

    // ---------------------
    // Logging configuration
    // ---------------------
    let format_pretty = fmt::format().with_thread_names(true).pretty();
    let format_json = fmt::format().with_thread_names(true).json();
    let log = &ac.application.log;
    let file_appender = match log.rotation {
        Rotation::Hourly => tracing_appender::rolling::hourly(&log.path, &log.file_name_prefix),
        Rotation::Daily => tracing_appender::rolling::daily(&log.path, &log.file_name_prefix),
        Rotation::Never => tracing_appender::rolling::never(&log.path, &log.file_name_prefix),
    };
    let (non_blocking, _guard1) = tracing_appender::non_blocking(file_appender);

    let filter = filter::LevelFilter::DEBUG;
    let (filter, reload_handle) = reload::Layer::new(filter);
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::Layer::default().event_format(format_pretty)) // console
        .with(
            fmt::Layer::default()
                .with_writer(non_blocking)
                .event_format(format_json),
        ) // log file
        .init();
    tracing::info!("Initialized logger: {}", log.level);

    let group = match conf.nacos.group.clone() {
        Some(g) => g,
        None => constants::DEFAULT_GROUP.to_owned(),
    };
    let listener = std::sync::Arc::new(SimpleConfigChangeListener::new(reload_handle));
    let _listen = cs.add_listener(conf.nacos.data_id.clone(), group, listener);
    match _listen {
        Ok(_) => tracing::info!("Add listener to configuration service is OK."),
        Err(err) => {
            tracing::error!("Failed to add listener to configuration service {:?}", err)
        }
    }
    // -------------------------
    // Register service to nacos
    // -------------------------
    let ne_listener = std::sync::Arc::new(SimpleNamingEventListener);
    _ = nacos_rust_client::register_nacos(&ac, ne_listener);

    for host in ac.application.hosts.iter() {
        tracing::info!(
            "Server is running on http://{}:{}",
            host,
            ac.application.port,
        );
    }
    // ------------------
    //  Start seb servers
    // ------------------
    let mut servers = Vec::with_capacity(2);
    for i in 0..ac.clone().application.hosts.len() {
        let server = HttpServer::new(App::new)
            .shutdown_timeout(30)
            .workers(ac.clone().application.workers)
            .bind((
                ac.clone().application.hosts[i].to_owned(),
                ac.clone().application.port,
            ))?
            .run();
        servers.push(server);
    }
    _ = future::try_join_all(servers).await;
    Ok(())
}

pub struct SimpleConfigChangeListener {
    reload_handle: Handle<LevelFilter, Registry>,
}

impl SimpleConfigChangeListener {
    pub fn new(reload_handle: Handle<LevelFilter, Registry>) -> Self {
        Self { reload_handle }
    }
}

impl ConfigChangeListener for SimpleConfigChangeListener {
    fn notify(&self, config_resp: ConfigResponse) {
        tracing::info!("Configuration changed: {}", config_resp);
        let ac = nacos_rust_client::parse_config_from_string::<Ac>(config_resp.content());
        nacos_rust_client::config_tracing(&self.reload_handle, ac.application.log);
    }
}

pub struct SimpleNamingEventListener;
impl NamingEventListener for SimpleNamingEventListener {
    fn event(&self, event: std::sync::Arc<NamingChangeEvent>) {
        tracing::info!("Heath check: {:?}", event);
    }
}
