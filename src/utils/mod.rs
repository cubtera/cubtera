pub mod helper;


pub fn logger_init(){
    env_logger::builder()
        .filter_level(
            std::env::var("CUBTERA_LOG")
                .unwrap_or_else(|_| "info".to_string())
                .parse()
                .unwrap_or(log::LevelFilter::Info),
        )
        .init();
}