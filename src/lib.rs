pub mod types;
use core::panic;
use error_chain::error_chain;
use nacos_sdk::api::{
    config::{ConfigChangeListener, ConfigResponse, ConfigService, ConfigServiceBuilder},
    constants,
    naming::{NamingEventListener, NamingService, NamingServiceBuilder, ServiceInstance},
    props::ClientProps,
};
use serde::de::DeserializeOwned;
use std::{env, sync::Arc};
use tracing::{level_filters::LevelFilter, Level};
use tracing_subscriber::{
    filter, fmt, prelude::__tracing_subscriber_SubscriberExt, reload, reload::Handle,
    util::SubscriberInitExt, Registry,
};
use types::{Ac, Env, Log, Nacos, Rotation};

error_chain! {
    foreign_links {
        Io(std::io::Error);
        NacosError(nacos_sdk::api::error::Error);
    }
}

pub const SERVICE_NAME: &str = env!("CARGO_PKG_NAME");

pub fn parse_config<T>(config_response: ConfigResponse) -> T
where
    T: DeserializeOwned,
{
    tracing::debug!("config_response: {:?}", config_response.content());
    let env: T = serde_yaml::from_str::<T>(config_response.content()).unwrap();
    env
}
pub fn parse_config_from_string<T>(content: &str) -> T
where
    T: DeserializeOwned,
{
    let env: T = serde_yaml::from_str::<T>(content).unwrap();
    env
}

/**
 * Read yaml configuration file from local filesystem
 * path: Relative path
 */
pub fn load_config<T>(path: &str) -> T
where
    T: DeserializeOwned,
{
    let yaml_str = read_yaml(path);

    let env: T = serde_yaml::from_str::<T>(&yaml_str)
        .unwrap_or_else(|_| panic!("Failed to read configuration file: {}", path));
    env
}
/**
 * Dserialize a yaml text to rust struct
 *
 * yaml_str: Yaml file text string
 */
pub fn deserialize_yaml<T>(yaml_str: String) -> T
where
    T: DeserializeOwned,
{
    serde_yaml::from_str::<T>(&yaml_str).unwrap()
}

/**
 * Convert a file to absolute path, and read it
 */
pub fn read_yaml(path: &str) -> String {
    let pwd = env::current_dir().unwrap_or_else(|_| panic!("Can not get current directory path."));
    let file = pwd.join(path);
    // tracing::debug!("Read file from: {}", file.as);
    std::fs::read_to_string(file).unwrap_or_else(|_| panic!("failure read file {}", path))
}

/**
 * Load application.yaml
 */
pub fn get_application_profile() -> Env {
    load_config::<Env>("application.yaml")
}

/**
 * Load application-{}.yaml, for example:
 *
 * - application-dev.yaml for development environment
 * - application-staging.yaml for staging environment
 * - application-production.yaml for production environment
 */
pub fn get_application_profile_active(active: &String) -> Nacos {
    let path = format!("application-{}.yaml", active);
    load_config::<Nacos>(&path)
}

pub struct RegisterService {
    cc: Box<dyn ConfigChangeListener>,
    ne: Box<dyn NamingEventListener>,
}

impl RegisterService {
    pub fn new(cc: Box<dyn ConfigChangeListener>, ne: Box<dyn NamingEventListener>) -> Self {
        Self { cc, ne }
    }

    pub fn set_cc(&mut self, cc: Box<dyn ConfigChangeListener>) {
        self.cc = cc;
    }

    pub fn set_ne(&mut self, ne: Box<dyn NamingEventListener>) {
        self.ne = ne;
    }
}
impl RegisterService {
    pub fn register_nacos(
        cc: impl ConfigChangeListener + 'static,
        ne: impl NamingEventListener + 'static,
    ) -> Result<Ac> {
        let env = &get_application_profile();
        let mut conf = get_application_profile_active(&env.profiles.active);
        conf.nacos.data_id = conf.nacos.data_id[..].replace("{}", &env.profiles.active);
        let client_props = ClientProps::new()
            .server_addr(conf.nacos.server_addr)
            .namespace(conf.nacos.namespace)
            .app_name(SERVICE_NAME);

        // Nacos Configuration Service
        let mut config_service = ConfigServiceBuilder::new(client_props.clone()).build()?;

        let group = match conf.nacos.group.clone() {
            Some(g) => g,
            None => constants::DEFAULT_GROUP.to_owned(),
        };
        tracing::info!("request configuration data id: {}", conf.nacos.data_id);
        let resp = config_service.get_config(conf.nacos.data_id.clone(), group.clone());

        let rr = match resp {
            Ok(r) => {
                tracing::info!("Get configuration from nacs successfully: {:?}", r);
                r
            }
            Err(err) => {
                tracing::error!("Incorrect configuration from nacos: {:?}", err);
                panic!("Incorrect configuration from nacos: {:?}", err);
            }
        };

        let ac = parse_config_from_string::<Ac>(rr.content());

        let _listen =
            config_service.add_listener(conf.nacos.data_id.clone(), group, std::sync::Arc::new(cc));
        match _listen {
            Ok(_) => tracing::info!("Add listener to configuration service is OK."),
            Err(err) => {
                tracing::error!("Failed to add listener to configuration service {:?}", err)
            }
        }

        ////////////////////////
        // Service Discovery
        ////////////////////////
        // 1. Create naming service
        let naming_service = NamingServiceBuilder::new(client_props).build()?;

        // 2. Subscribe
        let _subscribe_ret = naming_service.subscribe(
            SERVICE_NAME.to_owned(),
            conf.nacos.group.clone(),
            Vec::default(),
            std::sync::Arc::new(ne),
        );
        // 3. Register instances
        let mut instances = Vec::with_capacity(2);
        for host in ac.application.hosts.iter() {
            let instance = ServiceInstance {
                ip: host.to_owned(),
                port: ac.application.port as i32,
                weight: 1.0,
                ..Default::default()
            };
            tracing::info!(
                "Register service instance {}:{}",
                host.to_owned(),
                ac.application.port
            );
            instances.push(instance);
        }
        let _register_instance_ret = naming_service.batch_register_instance(
            SERVICE_NAME.to_owned(),
            conf.nacos.group.clone(),
            instances,
        );
        // 4. Get instances
        let instances_ret = naming_service.get_all_instances(
            SERVICE_NAME.to_owned(),
            conf.nacos.group,
            Vec::default(),
            false,
        );
        match instances_ret {
            Ok(instances) => tracing::info!("get_all_instances {:?}", instances),
            Err(err) => tracing::error!("naming get_all_instances error {:?}", err),
        }
        Ok(ac)
    }
}

/**
 * Register to nacos
 *
 * 1. Get application configs from nacos
 * 2. Init application with fetch configs
 * 3.
 */
pub fn get_config(// cc: impl ConfigChangeListener + 'static,
    // ne: impl NamingEventListener + 'static,
) -> Result<(Box<Ac>, Box<dyn ConfigService>)> {
    let env = &get_application_profile();
    let mut conf = get_application_profile_active(&env.profiles.active);
    conf.nacos.data_id = conf.nacos.data_id[..].replace("{}", &env.profiles.active);
    let client_props = ClientProps::new()
        .server_addr(conf.nacos.server_addr)
        .namespace(conf.nacos.namespace)
        .app_name(SERVICE_NAME);

    // Nacos Configuration Service
    let mut config_service = ConfigServiceBuilder::new(client_props).build()?;

    let group = match conf.nacos.group.clone() {
        Some(g) => g,
        None => constants::DEFAULT_GROUP.to_owned(),
    };
    tracing::info!("request configuration data id: {}", conf.nacos.data_id);
    let resp = config_service.get_config(conf.nacos.data_id, group);

    let cr = match resp {
        Ok(c) => {
            tracing::info!("Get configuration from nacs successfully: {:?}", c);
            c
        }
        Err(err) => {
            tracing::error!("Incorrect configuration from nacos: {:?}", err);
            panic!("Incorrect configuration from nacos: {:?}", err);
        }
    };
    let ac = parse_config_from_string::<Ac>(cr.content());
    Ok((Box::new(ac), Box::new(config_service)))
}

pub fn register_nacos(ac: &Ac, ne: Arc<impl NamingEventListener>) -> Result<()> {
    let env = &get_application_profile();
    let mut conf = get_application_profile_active(&env.profiles.active);
    conf.nacos.data_id = conf.nacos.data_id[..].replace("{}", &env.profiles.active);
    let client_props = ClientProps::new()
        .server_addr(conf.nacos.server_addr)
        .namespace(conf.nacos.namespace)
        .app_name(SERVICE_NAME);
    ////////////////////////
    // Service Discovery
    ////////////////////////
    // 1. Create naming service
    let naming_service = NamingServiceBuilder::new(client_props).build()?;

    // 2. Subscribe
    let _subscribe_ret = naming_service.subscribe(
        SERVICE_NAME.to_owned(),
        conf.nacos.group.clone(),
        Vec::default(),
        ne,
    );
    // 3. Register instances
    let mut instances = Vec::with_capacity(2);
    for host in ac.application.hosts.iter() {
        let instance = ServiceInstance {
            ip: host.to_owned(),
            port: ac.application.port as i32,
            weight: 1.0,
            ..Default::default()
        };
        tracing::info!(
            "Register service instance {}:{}",
            host.to_owned(),
            ac.application.port
        );
        instances.push(instance);
    }
    let _register_instance_ret = naming_service.batch_register_instance(
        SERVICE_NAME.to_owned(),
        conf.nacos.group.clone(),
        instances,
    );
    // 4. Get instances
    let instances_ret = naming_service.get_all_instances(
        SERVICE_NAME.to_owned(),
        conf.nacos.group,
        Vec::default(),
        false,
    );
    match instances_ret {
        Ok(instances) => tracing::info!("get_all_instances {:?}", instances),
        Err(err) => tracing::error!("naming get_all_instances error {:?}", err),
    }

    Ok(())
}

pub fn init_tracing(log: &Log) -> Handle<LevelFilter, Registry> {
    let format = fmt::format()
        // .with_level(false) // don't include levels in formatted output
        // .with_target(false) // don't include targets
        // .with_thread_ids(true) // include the thread ID of the current thread
        .with_thread_names(true) // include the name of the current thread
        .compact(); // use the `Compact` formatting style.

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
        .with(fmt::Layer::default().event_format(format.clone()))
        .with(fmt::Layer::default().with_writer(non_blocking).event_format(format))
        .init();

    tracing::info!("Initialized logger: {}", log.level);
    reload_handle
}

/**
 * Dynamic change log level
 */
pub fn config_tracing(reload_handle: &Handle<LevelFilter, Registry>, log: Log) {
    // pub fn config_tracing(log: Log) {
    tracing::info!("Logger level changed: {}", log.level);
    let filter = match log.level.as_str() {
        "TRACE" | "trace" => filter::LevelFilter::from_level(Level::TRACE),
        "DEBUG" | "debug" => filter::LevelFilter::from_level(Level::DEBUG),
        "INFO" | "info" => filter::LevelFilter::from_level(Level::INFO),
        "WARN" | "warn" => filter::LevelFilter::from_level(Level::WARN),
        "ERROR" | "error" => filter::LevelFilter::from_level(Level::ERROR),
        _ => filter::LevelFilter::from_level(Level::INFO),
    };
    _ = reload_handle.modify(|f| *f = filter);
}
