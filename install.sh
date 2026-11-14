#!/usr/bin/env bash
# Compila em release e instala o Chube só para o usuário (sem sudo): o binário em
# ~/.local/bin, o ícone e o .desktop que o põe no launcher e no dock. No GNOME, também a
# extensão desfoque-chube@pedro (vidro fosco atrás da janela), que vale depois de relogar.
# Rodar de novo atualiza a instalação; ./install.sh --uninstall remove.
set -euo pipefail
cd "$(dirname "$0")"

bin="$HOME/.local/bin/chube"
# O nome do .desktop é o app_id da janela: é assim que o dock liga a janela ao atalho.
desktop="$HOME/.local/share/applications/com.pedro.Chube.desktop"
folder_desktops=(
    "$HOME/.local/share/applications/com.pedro.Chube-claude.desktop"
    "$HOME/.local/share/applications/com.pedro.Chube-codex.desktop"
)
icon="$HOME/.local/share/icons/hicolor/scalable/apps/com.pedro.Chube.svg"
extension_id="desfoque-chube@pedro"
extension="$HOME/.local/share/gnome-shell/extensions/$extension_id"

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -fv "$bin" "$desktop" "$icon" "${folder_desktops[@]}"
    update-desktop-database "$(dirname "$desktop")" 2>/dev/null || true
    rm -rfv "$extension"
    exit 0
fi

cargo build --release
# Copia e renomeia: sobrescrever direto falharia com o Chube instalado aberto ("text file busy").
install -Dm755 target/release/chube "$bin.new"
mv -f "$bin.new" "$bin"
install -Dm644 res/com.pedro.Chube.svg "$icon"
# Caminho absoluto: aberto pelo launcher, o PATH não tem ~/.local/bin.
mkdir -p "$(dirname "$desktop")"
sed "s|@BIN@|$bin|" res/com.pedro.Chube.desktop > "$desktop"
# Entradas separadas: o Files mostra cada agente no seletor de "Open with".
for agent in claude codex; do
    sed "s|@BIN@|$bin|" "res/com.pedro.Chube-$agent.desktop" > "$(dirname "$desktop")/com.pedro.Chube-$agent.desktop"
done
update-desktop-database "$(dirname "$desktop")" 2>/dev/null || true

# Extensão do GNOME: copiada sempre (não custa nada fora do GNOME) e ligada pela lista do
# gsettings, porque o `gnome-extensions enable` não acha uma extensão nova antes de relogar.
mkdir -p "$extension"
cp gnome/$extension_id/{extension.js,desfoque.js,metadata.json} "$extension/"
if command -v gsettings >/dev/null && gsettings get org.gnome.shell enabled-extensions >/dev/null 2>&1; then
    enabled=$(gsettings get org.gnome.shell enabled-extensions)
    if [[ "$enabled" != *"'$extension_id'"* ]]; then
        if [[ "$enabled" == "@as []" ]]; then
            enabled="['$extension_id']"
        else
            enabled="${enabled%]}, '$extension_id']"
        fi
        gsettings set org.gnome.shell enabled-extensions "$enabled"
        echo "Extensão $extension_id ligada: vale depois de sair e entrar de novo na sessão."
    fi
fi

echo "Instalado: $bin"
echo "Atalho:    $desktop"
