#!/usr/bin/env bash
# Compila em release e instala o Chube só para o usuário (sem sudo): o binário em
# ~/.local/bin, o ícone e o .desktop que o põe no launcher do Omarchy.
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

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -fv "$bin" "$desktop" "$icon" "${folder_desktops[@]}"
    update-desktop-database "$(dirname "$desktop")" 2>/dev/null || true
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

echo "Instalado: $bin"
echo "Atalho:    $desktop"
