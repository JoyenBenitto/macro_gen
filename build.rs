fn main() {
    if pkg_config::probe_library("ngspice").is_err() {
        panic!(
            "could not find the 'ngspice' pkg-config library (ngspice.pc); \
             install the libngspice development package (e.g. libngspice0-dev on Debian/Ubuntu)"
        );
    }
}
