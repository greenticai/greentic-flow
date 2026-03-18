#[test]
fn modules_exist_and_can_compile() {
    #[allow(unused)]
    fn _touch() {
        use greentic_interfaces_wasmtime::component_v0_5::Component as _ComponentShim;
        let _instantiate = |_engine: &wasmtime::Engine,
                            bytes: &[u8]|
         -> anyhow::Result<wasmtime::component::Component> {
            _ComponentShim::instantiate(_engine, bytes)
        };

        #[cfg(feature = "control-helpers")]
        {
            use greentic_interfaces_wasmtime::component_v0_5::ControlHost;
            fn _takes_control_host(_: &dyn ControlHost) {}
            let _ = _takes_control_host;
        }
    }

    _touch();
}
