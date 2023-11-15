fn main() {
    env_logger::init();
    pollster::block_on(minecraft_clone::run())
}
