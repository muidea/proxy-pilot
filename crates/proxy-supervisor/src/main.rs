fn main() {
    proxy_runtime::init_local_offset();
    let result = tokio::runtime::Runtime::new()
        .expect("Tokio runtime")
        .block_on(async {
            let root = proxy_supervisor::ipc::data_arg()?;
            proxy_supervisor::ipc::serve(&root).await
        });
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
