//! O modo filtro do binário (`chube --filtro-cores -- <comando>`), rodado de verdade.

#[test]
fn filtro_roda_o_comando_troca_a_cor_e_devolve_o_codigo() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_chube"))
        .args(["--filtro-cores", "--", "sh", "-c", "printf '\\033[48;2;55;55;55mok'; exit 3"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\x1b[48;5;237;38;5;255mok"), "{stdout:?}");
}
