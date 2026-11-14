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

## Migração para GTK4 + VTE (2026-11-14)

**Por quê:** no libcosmic (iced), cada evento de mouse custa CPU fixa e cada mudança de hover redesenha a janela inteira,
inclusive o texto dos terminais. Medido com um mouse virtual (uinput, 4 s de movimento, ticks de CPU de 10 ms, mesmo
`chats.json`, cursor posto com `hyprctl dispatch 'hl.dsp.cursor.move({ x = …, y = … })'`; o `dispatch movecursor X Y`
antigo não faz nada com a configuração em Lua): libcosmic 63 na lista e 36 no terminal; GTK 43 na lista e 22 no terminal;
os dois 5 parados. O que sobra na lista do GTK é o redesenho da linha a cada troca de hover (~1 ms; sem hover, 30). Fazer o
hover por uma camada com opacidade em vez do `:hover` do CSS não mudou nada (medido). Trocar só para o iced original não
resolveria (mesma arquitetura).

**Como ficou:** `app.rs` guarda o estado e trata `Message`s (`App::update` → `Task`); `ui.rs` monta os widgets e, depois de
cada mensagem, `Ui::sync` os deixa iguais ao estado (a lista, as abas, o painel de uso e o de sessões só são refeitos
quando o que mostram muda). Os terminais são `vte4::Terminal` (`terminal.rs`), um por chat e por aba, nunca recriados:
viram páginas de `gtk::Stack`. As tarefas (`task.rs`) rodam num runtime do tokio e voltam pelo canal do laço principal.
A instância única é a do `GApplication` (`HANDLES_COMMAND_LINE`): a linha de comando de uma segunda chamada chega à janela
aberta, com os caminhos relativos resolvidos na pasta de quem chamou (`cli::parse_in`). Os atalhos são lidos na fase de
captura da janela (`shortcuts::find`, com GDK), antes do VTE. `src/term/` (cosmic-term) e `commands.rs` (sugestões do campo
de mensagem, que estava fora da tela desde o `29b1144`) saíram; estão no histórico.

**Portado (etapas 1 a 3):** tudo o que a versão libcosmic fazia, menos o fundo das mensagens do `claude` recolorido pelo
tema (o VTE não remapeia uma cor RGB; o `claude` pinta a faixa com `rgb(55,55,55)`) e o campo de mensagem.
Merge da `gtk` na `omarchy` e instalado pelo `./install.sh` em 2026-11-14.

**Testar sem mexer no app aberto:** `XDG_STATE_HOME=<pasta> dbus-run-session -- target/release/chube`, com o link
`<pasta>/omarchy` → `~/.local/state/omarchy`. O `dbus-run-session` separa a instância única; ao fechar, a cópia do
`xdg-desktop-portal-hyprland` dessa sessão cai e gera uma notificação de crash (inofensiva). Para dirigir a janela de
teste: o teclado virtual só chega depois de um clique nela; o Ctrl direito no layout br (thinkpad) é `/`, use o esquerdo;
o painel de nova sessão abre com Ctrl+Shift+N mesmo sem chat (estado vazio evita digitar num `claude` por engano).

Rascunho da interface: `chub_racunho.png` (na raiz).

Projeto em português (Brasil). Comentários e UI em PT-BR; identificadores de código em inglês.

## Stack

- **Rust** (edition 2024) + **GTK4** (`gtk4` 0.11, feature `v4_18`) + **VTE** para GTK4 (`vte4` 0.10, feature `v0_78`).
  Pacotes do Arch: `gtk4` e `vte4`. Até 2026-11-14 era libcosmic (veja "Migração para GTK4 + VTE").
- **Claude Code** (CLI `claude`, testado na 2.1.283): cada chat é o `claude` interativo (a TUI) rodando num VTE.
- **tokio** (runtime de 2 threads em `task.rs`) para os processos e o SSH; `async-channel` liga as tarefas ao laço do GTK.
- **Licença GPL-3.0-only** (vinha do código do cosmic-term, que saiu; mudar é decisão do dono do projeto).

## Comandos

- `cargo run` — roda em modo debug.
- `cargo build --release` — binário otimizado em `target/release/chube`.
- `./install.sh` — compila em release e instala para o usuário: binário em `~/.local/bin/chube`, ícone
  (`res/com.pedro.Chube.svg` → `~/.local/share/icons/hicolor/scalable/apps/`) e `.desktop` (`res/com.pedro.Chube.desktop`
  → `~/.local/share/applications/com.pedro.Chube.desktop`, com o caminho absoluto do binário). Rodar de novo atualiza;
  `./install.sh --uninstall` remove. O nome do `.desktop` é o `APP_ID` da janela, que é como o dock liga os dois.
- `cargo test` — testes do leitor do `/usage` e do `/context` (`src/usage.rs`), de `process_running` e `short_tokens` (`src/app.rs`),
  dos atalhos (`src/shortcuts.rs`), da linha de comando (`src/cli.rs`), do agrupamento da lista de chats (`src/groups.rs`),
  da lista de chats gravada (`src/restore.rs`), da paleta (`src/themes.rs`), do tema, da fonte, dos limites e da conta do
  Omarchy (`src/omarchy.rs`), dos hosts, das aspas, do comando do tmux e da lista da VPS (`src/remote.rs`), dos chats da VPS,
  do tmux de fora e do fim da tela (`src/chat.rs`), do comando, do título, do status pelo título e do modo em `src/claude.rs`.
- Opções: `chube [--agent omarchy|claude|codex] [--model <modelo>] [--open-directory <pasta>] [-- <pastas>…]`. `--agent omarchy`
  usa o agente padrão do Omarchy no modo do `omarchy agent`; `--model` vale para os chats novos desse agente.

> Testes que falam com o modelo gastam o limite do plano Pro: use `--model haiku` e prompts curtos.

## Estrutura

```
src/
  main.rs       Limpa as variáveis de sessão herdadas de um Claude Code pai e chama `app::run`
  cli.rs        Argumentos (--agent omarchy|claude|codex, --model, --open-directory); `parse_in` resolve os
                caminhos na pasta de quem chamou (a linha de comando chega à instância aberta pelo GApplication)
  app.rs        Estado (App) e `Message`s: chats, painéis, abas, poll do status, uso, VPS, modo, tema;
                `run`/`start` ligam o GApplication, a janela e o laço das mensagens
  ui.rs         Janela GTK: lista agrupada | chat (pilha de terminais e painéis) | terminal com abas; painel
                "uso", seletor de modo, CSS gerado da paleta; `Ui::sync` copia o estado do App para os widgets
  panels.rs     Painéis no lugar do chat: nova sessão (montado uma vez) e sessões salvas (refeito por versão)
  terminal.rs   VTE: criação, tema (fonte e cores do Omarchy), título (termprop `xterm.title`) e saída do processo
  task.rs       `Task` (futuro no tokio ou mensagem depois de uma espera) e o runtime das tarefas
  chat.rs       Chat (terminal com o agente, status/título/contexto/modo, modo do Omarchy, host e tmux da VPS),
                Status; new / resume / fork / reopen / attach e o comando do terminal (`Chat::command`)
  agent.rs      Agent::{Claude,Codex}: comando local e só os argumentos (para a VPS)
  codex.rs      Comando e histórico do Codex
  omarchy.rs    O que vem do Omarchy: cores do tema, modo vidro, fonte do foot, fonte mono do fontconfig,
                agente padrão, limites de uso e conta ativa (só leitura)
  remote.rs     Sessões na VPS: hosts do ~/.ssh/config, comando ssh + tmux, lista das sessões e dos tmux de lá,
                conexão de estado (`poll`)
  claude.rs     Comando do `claude` interativo, caminho do binário, título vindo do terminal, Mode
  shortcuts.rs  Atalhos de teclado (`find`, com as teclas do GDK)
  sessions.rs   Sessões salvas (~/.claude/projects), status de um `claude` pelo pid (~/.claude/sessions)
                e contexto/modelo pelo fim do .jsonl
  dirs.rs       Resolve o diretório digitado: ".", "~", caminhos, atalhos do fshort (~/.shortcuts)
  groups.rs     Agrupa a lista de chats: categoria (agente omarchy > local > vps)
                e, dentro dela, pasta; os atalhos de chat de cima/de baixo seguem essa ordem
  usage.rs      Limites do plano via `/usage`
  restore.rs    Lista de chats abertos, gravada a cada mudança e reaberta ao iniciar
  themes.rs     Paleta do tema do Omarchy (num static, relida quando o tema muda)
```

## Integração com o Claude Code (`src/claude.rs`)

Cada chat é um terminal (o mesmo widget do painel da direita) rodando:

```
env -u ANTHROPIC_API_KEY claude (--session-id <uuid> | --resume <id> | --resume <id> --fork-session)
       [--model <m>] [-- <prompt>]
```

- A conversa, as permissões, os comandos `/…` etc. são da própria TUI do Claude Code; o app só escreve no PTY,
  como um teclado (seletor de modo).
- Todos os chats ficam vivos ao mesmo tempo; a lista "# chats" só troca qual terminal aparece na seção central.
- `ANTHROPIC_API_KEY` sai do ambiente pelo `env -u`: com ela, o Claude Code cobraria pela API em vez do plano.
  O `env` faz `exec`, então o pid que o VTE devolve (`Chat::pid`) é o do `claude`.
- `main.rs` apaga as variáveis que um Claude Code põe nos filhos (`CLAUDECODE`, `CLAUDE_CODE_CHILD_SESSION`…).
  Herdadas (Chube aberto de dentro de uma sessão, ex.: `cargo run` pelo Claude), o `claude` dos chats se acha
  aninhado e **não grava a sessão** ("Transcript saving is off"): sem `.jsonl`, sem `~/.claude/sessions/<pid>.json`.
- O `claude` é procurado no PATH e, se não houver, em `~/.local/bin/claude` (o launcher não põe `~/.local/bin` no PATH).
- Fechar o app fecha os PTYs, e o `claude` recebe SIGHUP. Fechar um chat manda o SIGHUP direto (`app::hang_up`): o VTE
  só fecharia o PTY quando o GTK liberasse o widget.
- Quando o `claude` sai (`/exit`), o chat fica "encerrado" com a última saída na tela e o botão "Reabrir sessão"
  (`--resume` do mesmo id, no mesmo lugar da lista).
- O `×` do cabeçalho e o de cada item da lista "# chats" fecham o chat. Sem nenhum chat, a seção central diz como abrir
  um; o código usa `self.chats.get(self.focused)`, nunca indexação direta.
- O botão do terminal (``) leva a sessão do chat em foco para o terminal do Omarchy: confere se já existe o `.jsonl`
  (`sessions::session_file`; sessão nova sem mensagem não tem, e o `--resume` falharia), fecha o chat, espera o `claude`
  sair (`/proc/<pid>/stat`, zumbi conta como encerrado, até `EXIT_WAIT`) e roda
  `omarchy-launch-tui --app-id=org.omarchy.agent sh -c 'cd "$0" && exec "$@"' <pasta> env -u ANTHROPIC_API_KEY claude --resume <id> […]`
  (janela de agente, como as do `omarchy agent`; o `uwsm-app` não leva a pasta atual, por isso o `sh`). Num chat da VPS, a
  janela nova entra no mesmo tmux. Falhas aparecem como aviso (`App::notice`) abaixo do cabeçalho do chat.

### Chats restaurados ao abrir (`src/restore.rs`)
- O app abre sem chat novo: volta a lista da última execução, gravada em `$XDG_STATE_HOME/chube/chats.json`
  (padrão `~/.local/state/chube/chats.json`): id da sessão, pasta, título, `--model`, sub-chat de quem e o chat em foco.
- Gravada a cada mudança, não só ao fechar (logout ou `kill` não passam pelo fechamento): `App::update`
  chama `App::handle` e depois `save_open_chats`, que compara com a última gravação e só escreve se mudou
  (arquivo temporário + `rename`). Sub-chat sem id conhecido ainda fica de fora.
- Os chats voltam em espera (`Status::Suspended`, `-` "em espera"): só o do foco abre com `--resume`, os outros
  quando forem selecionados (`App::wake_focused`, também chamado depois de cada mensagem). Evita subir N `claude` de uma vez.
- Sessão sem `.jsonl` (nenhuma mensagem, ou apagada pelo Claude Code) volta como chat novo na mesma pasta: o `--resume` falharia.
- Testar sem mexer na lista real: `XDG_STATE_HOME=<pasta> dbus-run-session -- target/release/chube`, com um link `<pasta>/omarchy` → `~/.local/state/omarchy`
  (o Chube lê o tema e os limites do Omarchy no mesmo `XDG_STATE_HOME`; sem o link, cai no Starship de fábrica e no `/usage`).

### Modo de permissão
- Lido na tela (`chat::screen_tail` sobre o texto do VTE, últimas 4 linhas não vazias): `manual mode on`, `accept edits on`, `plan mode on`,
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

- Um `EventControllerKey` na janela, na fase de captura, vê as teclas antes do terminal em foco; os atalhos param ali e
  não chegam ao PTY. Só Ctrl e Shift contam (Caps Lock e Num Lock ficam de fora).
- Os números valem pela tecla física (código do GDK, evdev + 8), independentemente do layout.
- Ctrl+Tab: se o terminal do chat tem o foco, vai para a aba ativa; senão, para o chat (`Ui::switch_focus`).
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
- O modo do Omarchy (`auto`) só existe nesta máquina: o agente do Omarchy é do sistema do notebook. No painel de nova
  sessão, escolher "omarchy" volta o lugar para local e esconde os hosts; escolher um host troca o agente pelo que o
  Omarchy usaria. `Chat::with_place` e `Chat::restore` descartam `auto` num chat da VPS (inclusive gravado antes da regra).
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

## Terminal (VTE, `src/terminal.rs`)

O VTE (o terminal do GNOME Console e do Ptyxis) cuida do PTY, do desenho, da seleção, da área de transferência, dos
links e do método de entrada; o `echo_guard` e os ajustes de foco do código antigo do cosmic-term não são mais
necessários. O app ouve só o título (termprop `xterm.title`, o `window-title-changed` está obsoleto desde a 0.78) e a
saída do processo (`child-exited`). O fundo fica transparente (`set_clear_background(false)` e CSS) para o modo vidro.
O pid vem do callback do `spawn_async` (`Chat::pid`, numa `Rc<Cell>`). O texto da tela para o modo vem de
`text_format(Format::Text)`. A pasta de cada aba sai de `/proc/<pid>/cwd`, relida a cada `Poll`.

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

- **Temas (2026-11-14)**: seletor no botão de paleta do cabeçalho "# chats". Temas próprios em `palettes.rs`, opacos, para
  qualquer distro: Escuro (cinza `#1e1e1e` neutro, acento azul), Tokyo Night (padrão fora do Omarchy), Catppuccin Mocha e Latte, Gruvbox e Nord, com as cores oficiais
  (só o "apagado" do Tokyo Night, do Nord e do Latte é um tom mais forte, para ler; `palettes::tests` confere o contraste
  WCAG). "seguir o Omarchy" só aparece onde há `~/.local/state/omarchy/current/theme/colors.toml` e é o padrão lá. A escolha
  vai para `$XDG_CONFIG_HOME/chube/config.json` (`config.rs`); uma escolha que não existe nesta máquina volta ao padrão
  (`themes::resolve`). Com tema próprio, a conferência do tema do Omarchy não roda.
- **Cores** do tema atual do Omarchy (`~/.local/state/omarchy/current/theme/colors.toml`, `omarchy.rs`): fundo, texto,
  `muted` (apagado), acento, seleção e as ANSI (verde = "pronto", amarelo = aviso, vermelho = erro). No Starship do usuário o
  acento é `#7aa2f7` (overlay em `~/.config/omarchy/themes/starship/`) e o "verde" é laranja.
- **Tema ao vivo**: a cada 2 s (`Message::CheckTheme`, numa thread) compara `theme.generation` e as datas do
  `colors.toml`/`alacritty.toml`; mudou, `themes::reload`, `terminal::apply_theme` nos terminais e o CSS refeito.
- **Modo vidro**: a opacidade do bloco `# omarchy-glass` do `alacritty.toml` do tema (0,65) vai no fundo da janela
  (`window.chube` no CSS); cards, rolagens e terminais transparentes. O Hyprland desfoca o que fica atrás.
- **Fonte**: a família `monospace` do fontconfig (`fc-match`, hoje JetBrainsMono Nerd Font; muda com `omarchy font set`)
  no CSS e nos VTE. Tamanho da interface e dos terminais = o do foot (`font=…:size=9` → 12 px). Popovers são superfícies
  separadas e não herdam o CSS da janela: o menu de modos tem a fonte no seu próprio seletor.
- **Tokens do shell do Omarchy** (`shell.toml`): seções com borda de 1 px no `muted` a 35%, cantos retos; linha selecionada
  com fundo do texto a 8%, borda do texto a 25% e texto no acento; botões de texto com borda do texto a 40%; menu de modos
  com fundo opaco e borda no acento; dicas com borda do texto.
- **Ícones Nerd Font** (constantes `ICON_*` em `ui.rs` e `panels.rs`). Status: `` trabalhando (acento), `` precisa
  de você, `` pronto, `` iniciando, `` encerrado, `` em espera. Barras de uso em `ProgressBar` fina (cor do nível).
- Sem barra de título nem botões de janela: o Hyprland move e fecha (Super+W).
- **Painéis no lugar do chat** (`Panel`): "nova sessão" e "sessões" (abas `local` / hosts), páginas do mesmo `Stack` dos
  terminais; o cabeçalho do chat mostra o nome do painel e Esc volta.
- **Hover**: `:hover` no CSS. Cada troca de linha redesenha só a linha (~1 ms medido); uma camada com opacidade no lugar do
  CSS não mudou a CPU.

## Verificar a UI

O Claude não consegue usar o AT-SPI nesta janela, mas consegue dirigir uma instância de teste com mouse e teclado
virtuais (`/dev/uinput` tem ACL para o usuário; `uv run --with evdev`):
- abrir a instância de teste: `XDG_STATE_HOME=<pasta> dbus-run-session -- target/release/chube` num workspace livre
  (`hyprctl dispatch 'hl.exec_cmd("…", { workspace = "9 silent" })'`), com o link `<pasta>/omarchy`;
- posicionar o cursor: `hyprctl dispatch 'hl.dsp.cursor.move({ x = …, y = … })'` (coordenadas lógicas; o
  `dispatch movecursor X Y` antigo não faz nada com a configuração em Lua). Antes do clique, mexa 1 px: sem movimento o GTK
  não vê o ponteiro em cima do widget;
- teclado: só chega depois de um clique na janela; o Ctrl direito no layout br (thinkpad) é `/`, use o esquerdo;
- **não digite com um `claude` em foco** (vai como mensagem ao modelo); para testar o painel de nova sessão, use um estado
  vazio, sem chats;
- captura: `grim -g "<x>,<y> <w>x<h>"` com a janela no workspace visível (as notificações do Omarchy ficam por cima);
- medir CPU: ticks de `/proc/<pid>/stat` (campos 14 e 15) antes e depois de mover o mouse virtual por 4 s;
- para fechar só a instância de teste: o pid dela (**não** `pkill -f …chube`, que casa com o próprio shell do comando);
- lógica fora da UI (parsers, comandos, resolução de diretório): testes em Rust;
- script remoto do tmux: rodar o script gerado (`remote::command`) com `sh -c` num PTY, com `TMUX_TMPDIR` próprio;
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
- o terminal não tem menu de contexto, zoom, busca nem divisão em painéis;
- chats da VPS: o estado chega com até 3 s de atraso e some se a conexão de estado for negada; Codex na VPS só tem o status pelo título;
- a leitura do `/usage` depende do texto do Claude Code.


## Abrir pastas pelo Files (2026-11-11)

- `chube --agent claude|codex --open-directory <pasta>` cria uma sessão nova nessa pasta.
  Aceita também `-- <pastas>…`. Caminhos com espaços, acentos e caracteres de shell são argumentos literais. Caminhos
  inexistentes, arquivos e agentes inválidos são rejeitados (na primeira execução, antes de abrir a janela).
- Instância única pelo `GApplication` (`HANDLES_COMMAND_LINE`): uma segunda chamada manda a linha de comando à janela
  aberta pelo D-Bus; lá, `cli::parse_in` resolve os caminhos relativos na pasta de quem chamou
  (`ApplicationCommandLine::cwd`) e o app recebe `Message::OpenDirectories`. Sem pastas, só traz a janela para frente.
  A primeira execução restaura os chats anteriores e acrescenta o pedido externo, focando o último chat novo.
- `install.sh` instala e remove `com.pedro.Chube-{claude,codex}.desktop`, gerados de `res/`, com
  `Name=c-hub (claude)` / `c-hub (codex)`, `MimeType=inode/directory;`, `NoDisplay=true` e `%f`.
  As duas opções aparecem em "Open with" para pastas, sem acrescentar ícones ao launcher.
  `update-desktop-database` atualiza as associações; o Nautilus continua como aplicativo padrão para pastas.
  (Escrito no COSMIC/GNOME; no Omarchy o "Abrir com" do Nautilus não foi conferido.)
- Depois de atualizar, fechar e reabrir o c-hub para a instância atual passar a receber pedidos D-Bus.
  Testes de `cli.rs`: validação, caminhos relativos (também na pasta de quem chamou) e múltiplas pastas.
