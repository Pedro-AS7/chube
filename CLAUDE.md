# C-hub (chube) — v0.1.0

Nome visível: C-hub, desde 2026-10-23 (repositório github.com/Pedro-AS7/c-hub). Pasta, crate, binário `chube` e ID
`com.pedro.Chube` mantêm o nome antigo, para não quebrar o que já está instalado.

Chube é um app desktop para o **Omarchy** (Arch + Hyprland) que **gerencia sessões do Claude Code e do Codex**, nesta máquina
e **numa VPS por SSH**: lista de chats ativos, cada um com o `claude` ou o `codex` interativo num terminal integrado, chats no
**modo do `omarchy agent`**, sessões salvas para retomar, sub-chats (fork de sessão), painel de consumo do plano e **terminal
integrado com abas**. O visual segue o tema do Omarchy em uso. Usa o login existente de cada CLI; remove as chaves de API
herdadas do ambiente ao iniciar os chats.

Até 2026-11-13 era um app do COSMIC/GNOME, com 9 temas próprios e uma extensão do GNOME para o desfoque; a migração para o
Omarchy (branch `omarchy`) trocou isso pelo tema do Omarchy e acrescentou o agente do Omarchy e as sessões da VPS.

Rascunho da interface: `chub_racunho.png` (na raiz).

Projeto em português (Brasil). Comentários e UI em PT-BR; identificadores de código em inglês.

## Stack

- **Rust** (edition 2024) + **libcosmic** (git, fixado no commit `03d7dcb` de 2026-09-24 em `Cargo.toml`). O libcosmic usa um fork próprio do iced (`pop-os/iced`).
- Features do libcosmic: `winit`, `tokio`, `wayland`, `wgpu`, `multi-window`, `dbus-config`, `xdg-portal`, `single-instance`.
  **Não** ligue `a11y`: o cosmic-term avisa que ela derruba o seletor de arquivos (usado em "Escolher pasta…").
- **Claude Code** (CLI `claude`, testado na 2.1.283): cada chat é o `claude` interativo (a TUI) rodando num PTY.
- **Terminal**: `alacritty_terminal` 0.25 + `cosmic-text` 0.19, código portado do **cosmic-term**. `cosmic-text` precisa ser a
  mesma versão que o libcosmic usa (o terminal escreve no `font_system()` dele).
- **Licença GPL-3.0-only**, por causa do código do cosmic-term em `src/term/`.

## Comandos

- `cargo run` — roda em modo debug (a primeira compilação baixa e compila o libcosmic: alguns minutos).
- `cargo build --release` — binário otimizado em `target/release/chube`.
- `./install.sh` — compila em release e instala para o usuário: binário em `~/.local/bin/chube`, ícone
  (`res/com.pedro.Chube.svg` → `~/.local/share/icons/hicolor/scalable/apps/`) e `.desktop` (`res/com.pedro.Chube.desktop`
  → `~/.local/share/applications/com.pedro.Chube.desktop`, com o caminho absoluto do binário). Rodar de novo atualiza;
  `./install.sh --uninstall` remove. O nome do `.desktop` é o `APP_ID` da janela, que é como o dock liga os dois.
- `cargo test` — testes do leitor do `/usage` e do `/context` (`src/usage.rs`), de `process_running` e `short_tokens` (`src/app.rs`), dos atalhos (`src/shortcuts.rs`), da lista de chats gravada (`src/restore.rs`), da paleta (`src/themes.rs`),
  do tema, da fonte, dos limites e da conta do Omarchy (`src/omarchy.rs`), dos hosts, das aspas, do comando do tmux e da lista da VPS (`src/remote.rs`),
  dos chats da VPS e do tmux de fora (`src/chat.rs`), do comando, do título, do status pelo título e do modo em `src/claude.rs`,
  das sugestões de `/comandos` (`src/commands.rs`), do fundo das mensagens do `claude` (`src/term/terminal.rs`) e os que vieram do cosmic-term em `src/term/terminal_box.rs`.
- Opções: `chube [--agent omarchy|claude|codex] [--model <modelo>] [--open-directory <pasta>] [-- <pastas>…]`. `--agent omarchy`
  usa o agente padrão do Omarchy no modo do `omarchy agent`; `--model` vale para os chats novos desse agente.

> Testes que falam com o modelo gastam o limite do plano Pro: use `--model haiku` e prompts curtos.

## Estrutura

```
src/
  main.rs       Settings da janela e instância única via D-Bus
  cli.rs        Argumentos (--agent omarchy|claude|codex, --model, --open-directory) e envio à instância aberta
                (também limpa as variáveis de sessão herdadas de um Claude Code pai e define
                TERM/COLORTERM para os terminais: aberto pelo launcher, sem TERM o `claude` não tem cor)
  app.rs        App (cosmic::Application): layout em pane_grid (painel | chat | terminal), update/view,
                painéis de nova sessão e de sessões (no lugar do chat), painel "uso", leitura periódica
                do status dos chats (Message::Poll), conferência do tema do Omarchy (Message::CheckTheme)
  chat.rs       Chat (terminal com o agente, status/título/contexto/modo, modo do Omarchy, host e tmux da VPS),
                Status; new / resume / fork / reopen / attach e o comando do terminal (`Chat::command`)
  agent.rs      Agent::{Claude,Codex}: comando local e só os argumentos (para a VPS)
  codex.rs      Comando e histórico do Codex
  omarchy.rs    O que vem do Omarchy: cores do tema, modo vidro, fonte do foot, fonte mono do fontconfig,
                agente padrão, limites de uso e conta ativa (só leitura)
  remote.rs     Sessões na VPS: hosts do ~/.ssh/config, comando ssh + tmux, lista das sessões e dos tmux de lá
  claude.rs     Comando do `claude` interativo, caminho do binário, título vindo do terminal, Mode
  commands.rs   Comandos `/…` sugeridos no campo de mensagem (do Claude Code, do usuário e do projeto)
  shortcuts.rs  Atalhos de teclado (tabela `KEY_BINDS`, também passada aos terminais)
  sessions.rs   Sessões salvas (~/.claude/projects), status de um `claude` pelo pid (~/.claude/sessions)
                e contexto/modelo pelo fim do .jsonl
  dirs.rs       Resolve o diretório digitado: ".", "~", caminhos, atalhos do fshort (~/.shortcuts)
  usage.rs      Limites do plano via `/usage`
  restore.rs    Lista de chats abertos, gravada a cada mudança e reaberta ao iniciar
  themes.rs     Paleta do tema do Omarchy (num static, relida quando o tema muda) e o CosmicTheme dela
  term/         Terminal portado do cosmic-term (mod.rs substitui config, menu e atalhos de lá)
```

## Integração com o Claude Code (`src/claude.rs`)

Cada chat é um terminal (o mesmo widget do painel da direita) rodando:

```
env -u ANTHROPIC_API_KEY claude (--session-id <uuid> | --resume <id> | --resume <id> --fork-session)
       [--model <m>] [-- <prompt>]
```

- A conversa, as permissões, os comandos `/…` etc. são da própria TUI do Claude Code; o app só escreve no PTY,
  como um teclado (campo de mensagem, seletor de modo).
- Todos os chats ficam vivos ao mesmo tempo; a lista "# chats" só troca qual terminal aparece na seção central.
- `ANTHROPIC_API_KEY` sai do ambiente pelo `env -u`: com ela, o Claude Code cobraria pela API em vez do plano.
  O `env` faz `exec`, então o pid do PTY (`Terminal::pid`) é o do `claude`.
- `main.rs` apaga as variáveis que um Claude Code põe nos filhos (`CLAUDECODE`, `CLAUDE_CODE_CHILD_SESSION`…).
  Herdadas (Chube aberto de dentro de uma sessão, ex.: `cargo run` pelo Claude), o `claude` dos chats se acha
  aninhado e **não grava a sessão** ("Transcript saving is off"): sem `.jsonl`, sem `~/.claude/sessions/<pid>.json`.
- O `claude` é procurado no PATH e, se não houver, em `~/.local/bin/claude` (o launcher não põe `~/.local/bin` no PATH).
- Fechar o app fecha os PTYs, e o `claude` recebe SIGHUP.
- Quando o `claude` sai (`/exit`), o chat fica "encerrado" com a última saída na tela e o botão "Reabrir sessão"
  (`--resume` do mesmo id, no mesmo lugar da lista).
- O `[exit]` do cabeçalho e o `[x]` de cada item da lista "# chats" fecham o chat (descarta o `Terminal`: SIGHUP no `claude`). Sem nenhum chat, a seção central mostra
  "Nova sessão" / "Sessões salvas"; o código usa `self.chats.get(self.focused)`, nunca indexação direta.
- O botão do terminal (``) leva a sessão do chat em foco para o terminal do Omarchy: confere se já existe o `.jsonl`
  (`sessions::session_file`; sessão nova sem mensagem não tem, e o `--resume` falharia), fecha o chat, espera o `claude`
  sair (`/proc/<pid>/stat`, zumbi conta como encerrado, até `EXIT_WAIT`) e roda
  `omarchy-launch-tui --app-id=org.omarchy.agent sh -c 'cd "$0" && exec "$@"' <pasta> env -u ANTHROPIC_API_KEY claude --resume <id> […]`
  (janela de agente, como as do `omarchy agent`; o `uwsm-app` não leva a pasta atual, por isso o `sh`). Num chat da VPS, a
  janela nova entra no mesmo tmux. Falhas aparecem como aviso (`App::notice`) abaixo do cabeçalho do chat.

### Chats restaurados ao abrir (`src/restore.rs`)
- O app abre sem chat novo: volta a lista da última execução, gravada em `$XDG_STATE_HOME/chube/chats.json`
  (padrão `~/.local/state/chube/chats.json`): id da sessão, pasta, título, `--model`, sub-chat de quem e o chat em foco.
- Gravada a cada mudança, não só ao fechar (logout ou `kill` não passam pelo fechamento): `Application::update`
  chama `App::handle` e depois `save_open_chats`, que compara com a última gravação e só escreve se mudou
  (arquivo temporário + `rename`). Sub-chat sem id conhecido ainda fica de fora.
- Os chats voltam em espera (`Status::Suspended`, `-` "em espera"): só o do foco abre com `--resume`, os outros
  quando forem selecionados (`App::wake_focused`, também chamado depois de cada mensagem). Evita subir N `claude` de uma vez.
- Sessão sem `.jsonl` (nenhuma mensagem, ou apagada pelo Claude Code) volta como chat novo na mesma pasta: o `--resume` falharia.
- Testar sem mexer na lista real: `XDG_STATE_HOME=<pasta> cargo run`, com um link `<pasta>/omarchy` → `~/.local/state/omarchy`
  (o Chube lê o tema e os limites do Omarchy no mesmo `XDG_STATE_HOME`; sem o link, cai no Starship de fábrica e no `/usage`).

### Campo de mensagem (fora da tela desde o commit 29b1144; o código ficou)
- `text_editor` do libcosmic que cresce com o texto (até `COMPOSER_MAX_HEIGHT`, depois rola). Ao trocar de chat,
  o foco vai para o terminal do chat (`App::focus_chat`, pelo `chat_terminal_id`).
- Enter envia: o texto vai por bracketed paste (`Terminal::paste`) e o `\r` segue `SUBMIT_DELAY` depois — junto da
  colagem, a TUI trataria o Enter como texto. Assim `/model`, `/btw` etc. rodam como comandos (testado na 2.1.283).
- Teclas repassadas à TUI: Esc (interromper), Shift+Tab (modo), e, com o campo vazio, Enter e ↑/↓ (escolher opção
  num pedido de permissão). Shift+Enter quebra a linha; Tab completa o `/comando` sugerido.
- Sugestões: com o texto `/prefixo`, `commands::suggestions`; a lista (`commands::load`) é lida na primeira `/`.

### Modo de permissão
- Lido na tela (`term::screen_tail`, últimas 4 linhas): `manual mode on`, `accept edits on`, `plan mode on`,
  `auto mode on`, `bypass permissions on` (`Mode::from_screen`). Sem o rodapé na tela (pedido de permissão aberto),
  mantém o último lido. Relido a cada `Message::Poll`.
- Trocar no seletor manda um Shift+Tab (`\x1b[Z`) por vez e relê a tela (`MODE_STEP_DELAY`) até chegar ao pedido;
  desiste ao voltar ao modo inicial (o pedido não está no ciclo, ex.: "Sem permissões") ou após `MODE_MAX_STEPS`.

### Atalhos de teclado (`src/shortcuts.rs`)

| Tecla | Ação |
|---|---|
| Ctrl+Shift+N | Nova sessão (abre o painel) |
| Ctrl+Shift+1 / 2 / 3 | Mostrar/esconder painel, chat, terminal (chat e terminal não somem os dois) |
| Ctrl+Tab | Foco do teclado: terminal do chat ↔ terminal integrado (mostra a seção se estiver escondida; sem aba, abre uma) |
| Ctrl+Shift+↑ / ↓ | Chat de cima / de baixo na lista "# chats" (dá a volta nas pontas) |

- A mesma tabela (`KEY_BINDS`) vai para os `terminal_box` como `key_binds`: com o foco num terminal, essas teclas não
  chegam ao PTY. O app as recebe por `event::listen_with`, sem olhar se o evento foi capturado (os terminais capturam).
- Os números valem pela tecla física (`KeyBind::matches` com `physical_key`), independentemente do layout.
- Foco: os dois terminais na tela têm ids fixos (`chat_terminal_id`, `tab_terminal_id`); `operation::focus(id)` foca um e
  tira o foco dos outros; o Ctrl+Tab pergunta `operation::is_focused(chat_terminal_id)` para decidir o destino.
- A lista aparece na dica do `[?]` no cabeçalho "# chats"; as dicas dos botões mostram a tecla.

### Status, título e contexto
- A cada 1 s (`Message::Poll`, em `spawn_blocking`), para cada chat vivo: `~/.claude/sessions/<pid>.json` →
  `sessionId` e `status` (`busy` → trabalhando, `waiting` → precisa de você, `idle` → pronto). O `sessionId` muda
  com `/clear` e é assim que um sub-chat descobre o id novo.
- Só do chat em foco: os últimos 512 KB do `.jsonl` da sessão → tokens de contexto e modelo da última resposta.
  O tamanho da janela não é gravado: vem de `claude -p --model <modelo> "/context"` (local, 0 tokens,
  `**Tokens:** 11.3k / 1m`), uma vez por modelo (`usage::context_window`, cache em `App::context_windows`).
  Até responder, 200 mil; acima de 200 mil tokens, no mínimo 1 milhão.
- Título: o Claude Code define o título do terminal como `✳ Assunto` (spinner no lugar do `✳` enquanto trabalha);
  `claude::title_from_terminal` tira o símbolo e ignora `Claude Code`.

### Consumo do plano
- Os limites vêm de `~/.local/state/omarchy/agents/usage/{claude,codex}.json`, que o Omarchy atualiza
  (`omarchy agent usage update`): `limits[]` com `label`, `percent` (0–1) e `resetsAt`, mais `limitsFetchedAt` e
  `limitsStale`. Lidos a cada `REFRESH_EVERY` (2 min) e no `[↻]`; o rodapé mostra a hora da leitura do Omarchy.
- Sem esse arquivo, o Claude cai no `/usage`: `claude -p --no-session-persistence --output-format json "/usage"` roda
  **localmente** (0 tokens, ~2,5 s, não cria sessão). O texto é lido em `usage.rs`; se o formato mudar, "erro ao atualizar".
- A conta ativa (`Main (Pro)`) vem de `~/.local/state/omarchy/agents/accounts/claude.json`.

### Arquivos do Claude Code lidos pelo app
- `~/.claude/projects/<dir-codificado>/<session-id>.jsonl` — **todas** as sessões ficam aqui, de qualquer diretório.
  Linhas usadas: `cwd`, `custom-title` > `ai-title` > `last-prompt` (título na lista de sessões salvas) e a última
  `assistant` fora de `isSidechain` (`message.usage` e `message.model`). O Claude Code apaga sessões com mais de 30 dias (`cleanupPeriodDays`).
- `~/.claude/sessions/<pid>.json` — sessões abertas agora (`sessionId`, `status`, `cwd`); conferir `/proc/<pid>`.
  **Não leia os `.key`** do mesmo diretório (têm tokens).

## Integração com o Codex (2026-10-21)

- `src/agent.rs`: `Agent::{Claude,Codex}`, comando e localização do histórico de cada agente. O diálogo "Nova sessão"
  escolhe o agente e herda a seleção do chat em foco. Lista, cabeçalho e sessões salvas identificam o agente.
- `src/codex.rs`: `codex` / `codex resume <id>` / `codex fork <id>`, testados na CLI 0.163.0.
  Preserva modelo, permissões e sandbox configurados pelo usuário. O seletor de modo por Shift+Tab é só do Claude;
  o Codex usa sua própria TUI. Não carrega sugestões de comandos do Claude para chats Codex.
- Procura o Codex no PATH, em `~/.local/bin` e nas instalações do nvm (`NVM_DIR` ou `~/.nvm`, versões numéricas em
  ordem decrescente). Acrescenta o diretório encontrado ao PATH do processo para o wrapper npm achar o Node no launcher.
  `env -u` remove `OPENAI_API_KEY`, `CODEX_API_KEY`, `CODEX_THREAD_ID` e `CODEX_SESSION_ID` herdados.
- Passa `-c 'tui.terminal_title=["thread-id"]'` apenas ao processo do chat, sem editar `~/.codex/config.toml`.
  O título OSC do Codex 0.163.0 abrevia o UUID com `...`: o app resolve o prefixo (mínimo 16 caracteres) contra os
  arquivos de sessão e só aceita uma correspondência única. Não escolhe a sessão pela pasta, horário ou `--last`.
  Isso também acompanha `/new`, `/resume` e `/fork` dentro da CLI; uma leitura de disco antiga é descartada se o título mudou.
- Histórico em `$CODEX_HOME/sessions` (padrão `~/.codex/sessions`): metadados e primeira mensagem para a lista;
  nomes personalizados de `session_index.jsonl`; descarta subagentes e `source=exec`. Só lê arquivos de histórico,
  nunca `auth.json`. Lê os últimos 512 KiB para modelo, contexto, tamanho da janela, limites e início/fim de tarefas.
  Os limites são os últimos registrados nessa conversa; não são uma consulta ao vivo da conta. `/status` na CLI
  mostra os dados dela. Pedidos de aprovação do Codex não têm detecção separada de status no Chube.
- `restore::OpenChat` grava `agent`, `auto` (modo do Omarchy), `host` e `tmux` (VPS); arquivos antigos sem os campos continuam
  sendo chats locais do Claude, sem o modo. Identificadores desconhecidos
  de agente são ignorados. Chats novos sem histórico são gravados como rascunhos (`draft:<id>`) e voltam como chats
  novos do mesmo agente. Sub-chats sem id descoberto ainda ficam fora da restauração, como no Claude.
- Opção `--agent claude|codex` (padrão `claude`): agente inicial do diálogo e destino de `--model`. Ao escolher o outro
  agente, usa o modelo padrão da própria CLI. Chats restaurados mantêm o modelo gravado.
- Testes: comandos de criação/retomada/fork, prefixos OSC ambíguos, parser de histórico/contexto/limites, migração da
  lista antiga e preservação do agente ao restaurar, reabrir e criar sub-chat. A CLI real foi aberta num PTY com
  configuração temporária, sem enviar pedido ao modelo, confirmando o título OSC abreviado. O teste gráfico headless
  não pôde iniciar: o sandbox impede criar o socket do D-Bus. Diálogo, cliques, digitação e retomada/fork reais precisam
  de conferência manual no desktop.

## Terminal (`src/term/`)

`terminal.rs`, `terminal_box.rs` e `mouse_reporter.rs` vêm do cosmic-term (commit `9129277`, 2026-09-23) com poucas
mudanças, marcadas no topo de cada arquivo:
- eventos identificados por `TabId` em vez de `(pane, aba)`; `TerminalPaneGrid` removido; `Terminal::pid()`;
- copiar/colar (Ctrl+Shift+C/V) tratado dentro do widget (no cosmic-term é atalho do app);
- foco: o clique dá foco também no modo de mouse (a TUI do `claude` liga esse modo), clicar fora tira o foco
  (antes, o terminal do chat e o do painel ficavam os dois com foco e recebiam o mesmo texto) e o foco volta
  quando a janela recupera o dela (antes era preciso trocar de chat);
- `terminal_theme.rs` com as paletas COSMIC Dark/Light (sem uso) e `from_palette`, que monta a paleta do tema do Omarchy
  com o mesmo mapeamento do `alacritty.toml` gerado (preto = fundo, branco = texto, preto claro = `muted`);
- o fundo das mensagens do usuário na TUI do `claude` (`rgb(55,55,55)` no tema escuro dela, `rgb(240,240,240)` no claro;
  só elas usam esses cinzas, conferido na 2.1.303) vira a cor de seleção do tema do Omarchy com o texto claro dele
  (`USER_MESSAGE_BG`/`USER_MESSAGE_FG`, atômicos definidos em `term::set_background`).
- caracteres duplicados (2026-11-12): no GNOME/Wayland os caracteres que o IBus monta (ç, ã, á, à, â...) chegam
  como tecla pressionada **e** como `input_method::Event::Commit`, e saíam em dobro ("çç"). O `echo_guard.rs`
  (com testes) descarta o segundo quando o outro caminho trouxe o mesmo texto há até 50 ms. Só o caminho sem
  modificadores das teclas passa por ele. Testado só com `cargo test`; a digitação real fica com o usuário.

`term/mod.rs` substitui o que esses arquivos usavam do cosmic-term (`Config` com os valores padrão de lá, `MenuState`,
`Action`, que aqui é o `shortcuts::Shortcut`) e tem `spawn()`. Os eventos do PTY chegam por um canal único (`Message::TermEventTx` /
`TermEvent`), tratados em `App::handle_term_event` como no `Message::TermEvent` do cosmic-term.
Para trazer novidades do upstream, compare com o cosmic-term e reaplique só essas mudanças.

## Agente do Omarchy (2026-11-13)

- No painel de nova sessão, **omarchy (claude)** abre o agente padrão do Omarchy (`omarchy-default-agent`, lido ao abrir o app)
  como o `omarchy agent` faz: `claude --permission-mode auto` ou `codex --approve-for-me` (`Chat::auto`), na pasta `~/Work`
  quando o campo fica vazio. Se o agente padrão não for o Claude Code nem o Codex, a opção avisa e não cria.
  Sem chat em foco, é a opção escolhida de início.
- A conta (`omarchy agent account`) não precisa de nada do Chube: o `claude` e o `codex` do PATH são os wrappers do mise,
  que passam pelo `omarchy-agent-account-exec` e já apontam `CLAUDE_CONFIG_DIR`/`CODEX_HOME` para a conta ativa.
  Limitação: com uma conta que não seja a principal, o status e as sessões salvas continuam lidos de `~/.claude`.
- Na lista e no cabeçalho, o agente aparece como `omarchy · claude`. O modo é gravado na lista de chats e vale ao retomar,
  reabrir e no sub-chat. `chube --agent omarchy` faz o mesmo pela linha de comando.

## Sessões na VPS (2026-11-13, `src/remote.rs`)

- No painel de nova sessão, **onde** lista `local` e os hosts do `~/.ssh/config` (sem `*`/`?`). Na VPS, a pasta vai como
  digitada (vazio = `~`, a home de lá); `Chat::cwd` guarda a pasta de lá e o PTY local roda na home daqui.
- O chat roda `ssh -t <host> -- 'cd <pasta> || cd; exec tmux new-session -A -s chube-<8 hex> -e COLORTERM=truecolor bash -lc "<agente>" \; set-option …'`.
  O `-A` entra no tmux se ele ainda existe; senão cria rodando o agente (`export PATH="$HOME/.local/bin:$PATH"; env -u ANTHROPIC_API_KEY claude …`).
  Retomar uma sessão nova cujo histórico não foi gravado cai em `|| claude --session-id <mesmo id>`.
- Opções só nas sessões do Chube (`set-option -t`): sem barra de status, `mouse on` (a roda rola o histórico do tmux),
  `set-titles on` com `#{pane_title}` (o título do agente chega ao terminal daqui). A única opção global do servidor tmux é
  `terminal-features ',xterm-256color:RGB'` (acrescenta truecolor).
- **Fechar o chat só desconecta** (SIGHUP no `ssh`, o tmux continua). O botão `` (parar) do cabeçalho mata o tmux
  (`tmux kill-session`). Rede caída fecha o `ssh` (`ServerAliveInterval=15`, 4 tentativas): o chat fica "conexão encerrada"
  e "reconectar" volta ao mesmo tmux. O nome do tmux é gravado na lista de chats e reaproveitado ao reabrir o Chube.
- Estado (`remote::poll_stream`, assinatura por host em `App::subscription`): uma conexão SSH fixa por host roda lá um laço
  que, a cada 3 s, lê para cada sessão do tmux o `~/.claude/sessions/<pid>.json` (o `claude` é o processo do painel ou um
  filho dele: `tmux display-message '#{pane_pid}'` + `pgrep -P`) e a última resposta do `.jsonl` da sessão (`grep -o` do
  `model` e do começo do `usage`), e manda um bloco terminado por `E`. Traz id da sessão (sub-chat, `/clear`), status com
  "precisa de você", modelo e contexto, como nos locais; a janela de contexto do modelo vem do `/context` daqui.
  A conexão abre quando há um chat do Claude vivo lá cujo agente já subiu (a chave já foi liberada) e fecha quando não há
  mais (o laço morre no SIGPIPE do próximo `echo`). Se cai ou é negada, não reabre sozinha (seria uma janela atrás da
  outra): só quando um chat de lá (re)conecta ou a lista de lá carrega (`remote_poll_stopped`).
- O título (`claude::status_from_title`: `✳` pronto, spinner trabalhando) dá a resposta rápida entre as consultas, sem
  desfazer o "precisa de você". Títulos sem esse símbolo (o hostname que o tmux manda antes do agente) não viram o título do chat.
- O seletor de modo vale também na VPS: lê o rodapé da TUI na tela e manda Shift+Tab, que passam pelo tmux (menos nos
  chats que só entram num tmux de fora, que podem estar num shell).
- SSH: cada conexão autentica de novo (`ControlMaster=no`, `ControlPath=none`, mesmo que o `~/.ssh/config` tenha um mestre).
  Desde 2026-11-13 a chave do `vps-r1` entra no `ssh-agent` com confirmação (`AddKeysToAgent confirm`; askpass
  `~/.local/bin/ssh-confirmar` com o `hyprland-dialog`, veja o `~/.claude/sistema.md`): **cada conexão abre a janela
  "Permitir o uso da chave SSH?"**, e um processo qualquer não pega carona numa conexão do Chube. A senha da chave é digitada
  no terminal do primeiro chat da sessão; sem a chave no agente, as conexões sem terminal (`BatchMode=yes`) falham com
  "sem acesso…". Janelas por ação: chat novo, reconectar, lista da VPS, encerrar, abrir no terminal do Omarchy, e uma para
  a conexão de estado.
- Painel de sessões, aba do host: os tmux abertos lá (`tmux list-sessions`, clicar entra neles: `Chat::attach`, gravado como
  `tmux:<nome>`) e as últimas 150 sessões salvas do Claude Code de lá, lidas com `grep -o` (pasta e títulos, sem trazer o histórico).
- Testado: o script remoto num tmux local isolado (`TMUX_TMPDIR`): opções aplicadas, argumentos com espaço preservados,
  o tmux sobrevive ao SIGHUP e os títulos `✳`/spinner chegam ao terminal de fora. O script de estado, com um `claude` real
  num tmux local montado como o da VPS, achou o processo e leu id e status (também o laço da conexão fixa, com blocos a
  cada 3 s); o `grep` do histórico, sobre um `.jsonl` real, deu modelo e tokens. A confirmação do agente foi testada com
  uma chave descartável e um askpass automático. **Não testado contra a VPS de verdade**: `claude` no PATH de lá, primeira conexão, lista remota,
  estado, `kill`, reconectar.

## Convenções

- Comentários em PT-BR, curtos, explicando o porquê.
- Widgets com `widget::Column::new()` / `widget::Row::new()` (os helpers `widget::column()`/`row()` do iced exigem argumento),
  espaçamentos de `theme::spacing()`, seções dentro de `card()`.

## Visual (tema do Omarchy, desde 2026-11-13)

- **Cores** do tema atual do Omarchy (`~/.local/state/omarchy/current/theme/colors.toml`, `omarchy.rs`): fundo, texto,
  `muted` (apagado), acento, seleção e as ANSI (verde = "pronto", amarelo = aviso, vermelho = erro). No Starship do usuário o
  acento é `#7aa2f7` (overlay em `~/.config/omarchy/themes/starship/`) e o "verde" é laranja.
- **Tema ao vivo**: a cada 2 s (`Message::CheckTheme`, em `spawn_blocking`) compara `theme.generation` e as datas do
  `colors.toml`/`alacritty.toml`; mudou, `themes::reload` + `term::apply_theme` nos terminais + `set_theme` no libcosmic.
- **Modo vidro**: a opacidade do bloco `# omarchy-glass` do `alacritty.toml` do tema (0,65) vai no fundo da janela, pintado
  uma vez só no container da janela; cards sem fundo e terminais com `.opacity(0.0)`. O Hyprland desfoca o que fica atrás.
- **Fonte**: a família `monospace` do fontconfig (`fc-match`, hoje JetBrainsMono Nerd Font; muda com `omarchy font set`) é
  posta como monospace do cosmic-text em `themes::reload` (sem isso sairia a Noto/DejaVu). Tamanho da interface e dos
  terminais = o do foot (`font=…:size=9` → 12 px).
- **Tokens do shell do Omarchy** (`shell.toml`): seções com borda de 1 px no `muted` a 35%, cantos retos; linha selecionada
  com fundo do texto a 8%, borda do texto a 25% e texto no acento; botões de texto com borda do texto a 40%; menus suspensos
  (modo) com fundo opaco e borda de 2 px no acento, como os popups do shell; dicas com borda do texto.
- **Ícones Nerd Font** (constantes `ICON_*` em `app.rs`) no lugar dos botões `[+]`: novo, histórico, ajuda, esconder,
  fork, terminal, fechar, atualizar, servidor, parar. Status: `` trabalhando (acento), `` precisa de você, `` pronto,
  `` iniciando, `` encerrado, `` em espera. Barras de uso em traços `━` (cor do nível sobre a cor de seleção).
- Sem barra de título nem botões de janela: o Hyprland move e fecha (Super+W). Os cabeçalhos não arrastam mais a janela.
- **Painéis no lugar do chat** (`Panel`): "nova sessão" (agente, onde, pasta; rótulos em cima e opções em `flex_row`, que
  quebram linha numa seção estreita) e "sessões" (abas `local` / hosts). Substituem o diálogo e a gaveta do libcosmic.
- A faixa das mensagens do usuário na TUI do `claude` (`rgb(55,55,55)`) vira a cor de seleção do tema, com o texto claro.
- O `CosmicTheme` montado da paleta (cantos zero) vale para os widgets do libcosmic que sobraram (campo de texto, dicas,
  rolagens, popover).
- Funções async na UI via `cosmic::task::future(async { Message::… })`; trabalho de disco em `tokio::task::spawn_blocking`.
- `Message` precisa ser `Clone + Debug`: o `Terminal` fica no `Chat` (dentro de `Mutex`), nunca numa mensagem.

## Verificar a UI

O Claude **não consegue clicar nem digitar na janela** (o app não aparece no AT-SPI). Para validar:
- captura da janela, mesmo atrás de outras ou noutro workspace: `grim -T <stableId> saida.png`, com o `stableId` de
  `hyprctl clients -j` (o fundo sai cinza: é a transparência sem nada atrás). Não use `grim -g`: pega o que estiver na tela;
- painéis sem clicar: `CHUBE_DEBUG_PANEL=new` (ou `new:<host>`), `sessions` ou `sessions:<host>` abre o app já nele;
- chat sem clicar: `chube --agent omarchy --open-directory <pasta>` (sem prompt, não gasta limite);
- testar sem mexer na lista real: `XDG_STATE_HOME=<pasta>` com o link `<pasta>/omarchy` (veja "Chats restaurados");
- para fechar só a instância de teste: o pid que tem o `XDG_STATE_HOME` de teste em `/proc/<pid>/environ`
  (**não** `pkill -f …chube`, que casa com o próprio shell do comando). Se o Claude estiver rodando **dentro** de um Chube,
  abra uma segunda instância e feche só ela pelo pid;
- lógica fora da UI (parsers, comandos, resolução de diretório): testes em Rust;
- script remoto do tmux: rodar o script gerado (`remote::command`) com `sh -c` num PTY, com `TMUX_TMPDIR` próprio;
- cliques e digitação (troca de chat, painéis, criar chat, sub-chat, "reabrir sessão", terminais, seletor de modo, botão do
  terminal do Omarchy, atalhos de teclado, chats da VPS): pedir para conferir manualmente;
- bytes que a TUI entende: script Python com `pty.fork()` rodando o `claude` (comandos locais como `/help` não gastam tokens).

## Status do projeto

Versão 0.1.0. O roteiro inicial de 5 passos está completo (2026-09-27): esqueleto, chat real, sessões salvas/retomar/
sub-chat/permissões, painel de consumo e terminal integrado. Em seguida (2026-09-27), o chat em stream-json com
markdown e cartão de permissão foi trocado pelo `claude` interativo num terminal por chat. Repositório git criado (branch `main`), **ainda sem commits**.
Instalado pelo `./install.sh` (2026-09-27): aparece no launcher como "Chube" e pode ser fixado no dock.

Validado por captura de tela: o `claude` do chat abre com `--prompt`, responde, e status, modelo e contexto aparecem.
Na migração para o Omarchy (2026-11-13), validado por captura: tema Starship (acento azul, vidro desfocado, JetBrainsMono
Nerd Font), painéis de nova sessão e de sessões (local e a mensagem de "sem acesso" da VPS) e um chat no modo do Omarchy
(`auto mode on`, seletor em "automático"). Não testado com clique: os painéis, troca de tema ao vivo e qualquer chat da VPS.
Ainda não testado com clique: troca entre chats, painel de sessões, diálogo de nova sessão, sub-chat, "Reabrir sessão",
digitação nos terminais, fechar chat.

Limitações conhecidas:
- o status só vem de `~/.claude/sessions`, um arquivo interno do Claude Code (formato pode mudar);
- o terminal não tem menu de contexto, zoom, busca nem divisão em painéis (o cosmic-term tem);
- chats da VPS: o estado chega com até 3 s de atraso e some se a conexão de estado for negada; Codex na VPS só tem o status pelo título;
- a leitura do `/usage` depende do texto do Claude Code.


## Abrir pastas pelo Files (2026-11-11)

- `chube --agent claude|codex --open-directory <pasta>` cria uma sessão nova nessa pasta.
  Aceita também `-- <pastas>…`. Caminhos relativos são resolvidos no processo chamador; caminhos com espaços,
  acentos e caracteres de shell são argumentos literais. Caminhos inexistentes, arquivos e agentes inválidos
  são rejeitados antes de abrir a janela. O transporte D-Bus exige caminhos UTF-8.
- `cli.rs` valida os argumentos e implementa `CosmicFlags`. A feature `single-instance` e
  `cosmic::app::run_single_instance` encaminham pedidos à janela aberta. `App::dbus_activation` valida as
  pastas em `spawn_blocking` e entrega `Message::OpenDirectories`; o `update` grava os chats como de costume.
  Sem pastas, abrir pelo launcher só ativa a janela e não cria chat. A primeira execução restaura os chats
  anteriores e acrescenta o pedido externo, focando o último chat novo.
- `install.sh` instala e remove `com.pedro.Chube-{claude,codex}.desktop`, gerados de `res/`, com
  `Name=c-hub (claude)` / `c-hub (codex)`, `MimeType=inode/directory;`, `NoDisplay=true` e `%f`.
  As duas opções aparecem em "Open with" para pastas, sem acrescentar ícones ao launcher.
  `update-desktop-database` atualiza as associações; o Nautilus continua como aplicativo padrão para pastas.
  (Escrito no COSMIC/GNOME; no Omarchy o "Abrir com" do Nautilus não foi conferido.)
- Depois de atualizar, fechar e reabrir o c-hub para a instância atual passar a receber pedidos D-Bus.
  Testes de `cli.rs`: validação, caminhos relativos, múltiplas pastas e ida/volta pelo transporte dos argumentos.
