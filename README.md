# C-hub

Gerenciador de sessões do [Claude Code](https://claude.com/claude-code) e do [Codex](https://learn.chatgpt.com/docs/cli/reference) para o [Omarchy](https://omarchy.org)
(Arch + Hyprland), feito em Rust com o [libcosmic](https://github.com/pop-os/libcosmic). Segue o tema do Omarchy em uso.

![Rascunho da interface](chub_racunho.png)

- Lista de chats ativos, cada um com o `claude` ou `codex` interativo num terminal integrado
- **Agente do Omarchy**: abre o agente padrão (`omarchy default agent`) do jeito do `omarchy agent`, aprovando sozinho
- **Sessões numa VPS** por SSH, cada uma num tmux de lá: fechar o chat só desconecta, e dá para voltar depois (ou entrar pelo celular)
- Sessões salvas para retomar (daqui e da VPS), e sub-chats (um chat novo com o contexto do chat em foco)
- Painel de consumo do plano (os limites que o Omarchy mantém) e do contexto usado no chat em foco
- Terminal integrado com abas, ao lado do chat
- Visual do tema atual do Omarchy (cores, fonte, modo vidro), que muda junto com `omarchy theme set`

Escolha **omarchy**, **claude** ou **codex** e o lugar (**local** ou um host do `~/.ssh/config`) no painel "nova sessão". Chats dos agentes convivem na mesma lista, identificados pelo nome. Sessões salvas, retomada, sub-chats e restauração ao abrir preservam o agente, o modo e o lugar de origem.

Usa o login existente de cada CLI. O seletor de permissões do C-hub é exclusivo do Claude; no Codex, use os controles da própria TUI (`/permissions`, `/plan`, `/model`). Limites e contexto do Codex vêm do histórico local, quando disponíveis; `/status` mostra os dados da CLI.

> Projeto pessoal, sem ligação com a Anthropic ou a OpenAI.

## Requisitos

- Rust (edition 2024) e as dependências de compilação do libcosmic
- A CLI do agente desejado instalada e logada: Claude Code (`claude` no `PATH` ou em `~/.local/bin`) e/ou Codex (testado com 0.163.0; `codex` no `PATH`, em `~/.local/bin` ou instalado pelo nvm)
- Omarchy (Hyprland). Sem ele, usa as cores do tema Starship de fábrica
- Para a VPS: um host no `~/.ssh/config`, o `tmux` e o agente instalados e logados lá

## Instalar

```sh
./install.sh             # compila em release e instala para o usuário (sem sudo)
./install.sh --uninstall # remove
```

Instala o binário em `~/.local/bin/chube`, o ícone e o atalho `.desktop` no launcher.
Para só rodar: `cargo run --release`. Opções: `chube --agent omarchy|claude|codex` escolhe o agente das pastas abertas pela linha de comando ou pelo Files; `--model <modelo>` define o modelo dos chats novos desse agente. Para o outro agente, vale o modelo configurado na própria CLI.

## Licença

[GPL-3.0-only](LICENSE). O terminal em `src/term/` é adaptado do [cosmic-term](https://github.com/pop-os/cosmic-term)
(© System76, GPL-3.0-only).
