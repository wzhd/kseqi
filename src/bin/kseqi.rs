use std::error::Error;

fn main()-> Result<(), Box<dyn Error>> {
    env_logger::builder()
        .write_style(env_logger::WriteStyle::Always)
        .filter_level(log::LevelFilter::Info)
        .format_module_path(false)
        .format_target(false)
        .parse_default_env()
        .init();
    kseqi_desktop::run()
}
