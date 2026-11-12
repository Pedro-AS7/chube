# C-hub (chube) — v0.1.0

Nome visível: C-hub, desde 2026-10-23 (repositório github.com/Pedro-AS7/c-hub). Pasta, crate, binário `chube`, ID
`com.pedro.Chube` e a extensão `desfoque-chube@pedro` mantêm o nome antigo, para não quebrar o que já está instalado.

Chube é um app desktop para o COSMIC (Pop!_OS 24.04, Wayland) que **gerencia sessões do Claude Code e do Codex**: lista de chats ativos, cada um com o `claude` ou o `codex` interativo num terminal integrado, sessões salvas para retomar, sub-chats (fork de sessão), painel de consumo do plano e **terminal integrado com abas**. Usa o login existente de cada CLI; remove as chaves de API herdadas do ambiente ao iniciar os chats.

Rascunho da interface: `chub_racunho.png` (na raiz).

Projeto em português (Brasil). Comentários e UI em PT-BR; identificadores de código em inglês.

## Stack

- **Rust** (edition 2024) + **libcosmic** (git, fixado no commit `03d7dcb` de 2026-09-24 em `Cargo.toml`). O libcosmic usa um fork próprio do iced (`pop-os/iced`).
- Features do libcosmic: `winit`, `tokio`, `wayland`, `wgpu`, `multi-window`, `dbus-config`, `xdg-portal`.
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
- `cargo test` — testes do leitor do `/usage` e do `/context` (`src/usage.rs`), de `process_running` e `short_tokens` (`src/app.rs`), dos atalhos (`src/shortcuts.rs`), da lista de chats gravada (`src/restore.rs`), dos temas (`src/themes.rs`), do comando, do título e do modo em `src/claude.rs`,
  das sugestões de `/comandos` (`src/commands.rs`), do fundo das mensagens do `claude` (`src/term/terminal.rs`) e os que vieram do cosmic-term em `src/term/terminal_box.rs`.
- Opções: `chube [--model <modelo>] [--prompt <mensagem>]`. `--model` vale para os chats novos; `--prompt` é passado ao `claude`
  do primeiro chat, que já começa respondendo (útil para testar sem clicar).

> Testes que falam com o modelo gastam o limite do plano Pro: use `--model haiku` e prompts curtos.

## Estrutura

```
src/
  main.rs       Argumentos (--model, --prompt) e Settings da janela
                (também limpa as variáveis de sessão herdadas de um Claude Code pai e define
                TERM/COLORTERM para os terminais: aberto pelo launcher, sem TERM o `claude` não tem cor)
  app.rs        App (cosmic::Application): layout em pane_grid (painel | chat | terminal), update/view,
                diálogo de nova sessão, painel lateral de sessões salvas, painel "Uso", leitura periódica
                do status dos chats (Message::Poll)
  chat.rs       Chat (terminal com o `claude` + campo de mensagem, status/título/contexto/modo), Status;
                new / resume / fork / reopen
  claude.rs     Comando do `claude` interativo, caminho do binário, título vindo do terminal, Mode
  commands.rs   Comandos `/…` sugeridos no campo de mensagem (do Claude Code, do usuário e do projeto)
  shortcuts.rs  Atalhos de teclado (tabela `KEY_BINDS`, também passada aos terminais)
  sessions.rs   Sessões salvas (~/.claude/projects), status de um `claude` pelo pid (~/.claude/sessions)
                e contexto/modelo pelo fim do .jsonl
  dirs.rs       Resolve o diretório digitado: ".", "~", caminhos, atalhos do fshort (~/.shortcuts)
  usage.rs      Limites do plano via `/usage`
  restore.rs    Lista de chats abertos, gravada a cada mudança e reaberta ao iniciar
  themes.rs     Temas (sistema, fósforo, neon, gruvbox, vidro, fumê, transparente, grafite, preto): paletas,
                CosmicTheme de cada um, tema gravado, pedido de desfoque para a extensão do GNOME
  term/         Terminal portado do cosmic-term (mod.rs substitui config, menu e atalhos de lá)
gnome/
  desfoque-chube@pedro/  Extensão do GNOME Shell: vidro fosco atrás da janela do Chube
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
- `[term]` leva a sessão do chat em foco para o COSMIC Terminal: confere se já existe o `.jsonl`
  (`sessions::session_file`; sessão nova sem mensagem não tem, e o `--resume` falharia), fecha o chat, espera o `claude`
  sair (`/proc/<pid>/stat`, zumbi conta como encerrado, até `EXIT_WAIT`) e roda
  `cosmic-term --working-directory <pasta> -e env -u ANTHROPIC_API_KEY claude --resume <id> [--model <m>]`.
  O `cosmic-term` se separa e sai logo. Falhas aparecem como aviso (`App::notice`) abaixo do cabeçalho do chat.

### Chats restaurados ao abrir (`src/restore.rs`)
- O app abre sem chat novo: volta a lista da última execução, gravada em `$XDG_STATE_HOME/chube/chats.json`
  (padrão `~/.local/state/chube/chats.json`): id da sessão, pasta, título, `--model`, sub-chat de quem e o chat em foco.
- Gravada a cada mudança, não só ao fechar (logout ou `kill` não passam pelo fechamento): `Application::update`
  chama `App::handle` e depois `save_open_chats`, que compara com a última gravação e só escreve se mudou
  (arquivo temporário + `rename`). Sub-chat sem id conhecido ainda fica de fora.
- Os chats voltam em espera (`Status::Suspended`, `-` "em espera"): só o do foco abre com `--resume`, os outros
  quando forem selecionados (`App::wake_focused`, também chamado depois de cada mensagem). Evita subir N `claude` de uma vez.
- Sessão sem `.jsonl` (nenhuma mensagem, ou apagada pelo Claude Code) volta como chat novo na mesma pasta: o `--resume` falharia.
- Testar sem mexer na lista real: `XDG_STATE_HOME=<pasta> cargo run`.

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
| Ctrl+Shift+N | Nova sessão (abre o diálogo) |
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
- `claude -p --no-session-persistence --output-format json "/usage"` roda o `/usage` **localmente**: 0 tokens, ~2,5 s,
  não cria sessão salva. O texto (`Current session: 34% used · resets Sep 27, 12:50am (…)`) é lido em `usage.rs`.
  O formato não é oficial: se mudar, o painel mostra "Erro ao atualizar" — ajuste `parse_line` e os testes.

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
- `restore::OpenChat` grava `agent`; arquivos antigos sem o campo continuam sendo Claude. Identificadores desconhecidos
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
- `terminal_theme.rs` só com as paletas COSMIC Dark/Light;
- o fundo das mensagens do usuário na TUI do `claude` (`rgb(55,55,55)` no tema escuro dela, `rgb(240,240,240)` no claro;
  só elas usam esses cinzas, conferido na 2.1.303) vira uma faixa branca a 50% com texto preto em todos os temas
  (`USER_MESSAGE_BG`/`USER_MESSAGE_FG`). Sem isso, a mensagem quase não se distinguia.

- caracteres duplicados (2026-11-12): no GNOME/Wayland os caracteres que o IBus monta (ç, ã, á, à, â...) chegam
  como tecla pressionada **e** como `input_method::Event::Commit`, e saíam em dobro ("çç"). O `echo_guard.rs`
  (com testes) descarta o segundo quando o outro caminho trouxe o mesmo texto há até 50 ms. Só o caminho sem
  modificadores das teclas passa por ele. Testado só com `cargo test`; a digitação real fica com o usuário.
`term/mod.rs` substitui o que esses arquivos usavam do cosmic-term (`Config` com os valores padrão de lá, `MenuState`,
`Action`, que aqui é o `shortcuts::Shortcut`) e tem `spawn()`. Os eventos do PTY chegam por um canal único (`Message::TermEventTx` /
`TermEvent`), tratados em `App::handle_term_event` como no `Message::TermEvent` do cosmic-term.
Para trazer novidades do upstream, compare com o cosmic-term e reaplique só essas mudanças.

## Convenções

- Comentários em PT-BR, curtos, explicando o porquê.
- Widgets com `widget::Column::new()` / `widget::Row::new()` (os helpers `widget::column()`/`row()` do iced exigem argumento),
  espaçamentos de `theme::spacing()`, seções dentro de `card()`.

## Visual (estilo "terminal")

- Sem a barra de título do libcosmic (`core.window.show_headerbar = false`, `content_container = false`): o conteúdo
  ocupa a janela toda. O cabeçalho de cada seção (`section_header`) arrasta a janela e maximiza com clique duplo;
  minimizar/maximizar/fechar (`window_controls`) ficam no fim do cabeçalho da seção mais à direita (terminal, ou chat).
  O `mouse_area` do iced captura o clique esquerdo mesmo sem `on_press`: um `mouse_area` dentro do cabeçalho (o da roda do
  mouse nas abas do terminal) precisa repetir o `on_drag`/`on_double_click`, senão a área dele não arrasta a janela.
- Seções (`card()`): cantos retos, borda de 1 px em cinza claro (`border_color`), espaço de `space_xxs` entre elas e
  dentro delas. O fundo (`background_color`) é pintado **uma vez só**, no container da janela: os cards não têm fundo e os
  terminais usam `.opacity(0.0)` no fundo padrão. Assim, com o tema "frosted" (blur) do COSMIC, as seções ficam translúcidas
  como os espaços entre elas; pintar de novo dobraria o alfa e deixaria as seções opacas.
- Todo texto em fonte mono (`mono`, `bold`, `tinted`, tamanho `TEXT_SIZE`); cores do tema em `Colors`.
  Os cabeçalhos das seções têm a mesma altura (`HEADER_HEIGHT`, duas linhas, como o do chat), para as linhas de baixo
  ficarem alinhadas; é altura fixa no container (um `Space` de altura mínima dentro de uma `Row` não funcionou).
  Títulos como `# chats`; botões de texto sem fundo (`ascii_button`/`flat_button`): `[+]`, `[fork]`, `[term]`, `[exit]`, `[«]`.
- Status com símbolos da TUI (`status_glyph`): `✻` trabalhando, `?` precisa de você, `●` pronto, `○` iniciando, `×` encerrado.
- Barras de uso/contexto em ASCII no estilo do htop (`ascii_bar`: `[||||||      42%]`, verde/amarelo ≥ 70%/vermelho ≥ 90%).
  A quantidade de `|` vem da largura disponível (`widget::responsive`) dividida pela largura estimada do caractere
  (`TEXT_SIZE * MONO_ADVANCE`, 0,6 na Fira Mono/Noto Sans Mono). Evite `█`/`░`: a Fira Mono (mono padrão do COSMIC)
  não tem esses caracteres e a fonte de reserva tem outra largura.
- Seletor de modo: botão `[⏸ manual ▾]` com menu em `widget::popover` (`App::mode_menu`), no lugar do `dropdown`
  (que não aceita estilo). Abas do terminal desenhadas à mão (`chube ×`), sobre o mesmo `segmented_button::Model`,
  num `scrollable` horizontal (a roda do mouse vertical vira rolagem horizontal pelo `mouse_area`; aba nova rola até o fim).
  O nome é a última pasta do shell (`TabDir`, relida em `/proc/<pid>/cwd` a cada mudança de título, que o oh-my-zsh faz
  a cada prompt); a segunda aba na mesma pasta vira `chube 2`, a terceira `chube 3` (`App::terminal_tab_names`).
- Continuam no estilo padrão do COSMIC: o diálogo "Nova sessão" e a gaveta de sessões salvas (só os itens dela são mono).
- Menus suspensos (modo, tema) usam `menu_card`: como `card`, mas com fundo opaco (o `popover` não pinta fundo).
- Funções async na UI via `cosmic::task::future(async { Message::… })`; trabalho de disco em `tokio::task::spawn_blocking`.
- `Message` precisa ser `Clone + Debug`: o `Terminal` fica no `Chat` (dentro de `Mutex`), nunca numa mensagem.

### Temas (`src/themes.rs`)
- `[◐]` no cabeçalho "# uso" abre o menu: **sistema** (cores do COSMIC, como antes), **fósforo** (fundo `#000000` até no
  container da janela, bordas cinza `#5A5A5A`, texto verde `#33FF66`), **neon** (gradiente azul translúcido na diagonal,
  bordas ciano com brilho via `Shadow` do container, desfoque do COSMIC) e **gruvbox** (paleta Gruvbox dark, quente e opaca).
  Mais quatro iguais aos temas de mesmo nome da dock, do painel e dos menus do GNOME (`temas.json` das extensões
  `temas-*@pedro`): **vidro** (branco translúcido em gradiente), **fumê** (cinza escuro translúcido), **transparente**
  (sem fundo, só o desfoque mais escuro) e **grafite** (cinza opaco). ANSI do GNOME Terminal.
  E **preto**: fundo `#000000` opaco (sem desfoque), texto cinza-claro neutro, ANSI do GNOME Terminal com o preto puro.
- O escolhido fica em `$XDG_CONFIG_HOME/chube/theme` (padrão `~/.config/chube/theme`, uma palavra: `system`, `phosphor`,
  `neon`, `gruvbox`, `glass`, `smoke`, `transparent`, `graphite`, `black`) e é aplicado no `init`, antes de abrir terminais. O tema atual fica num `static` (`Theme::current`),
  lido pelas funções de estilo (`Colors::get`, `window_background`, `border_color`, `card`, `flat_button`).
- Cada tema próprio vira um `CosmicTheme` (`ThemeBuilder` com fundo, `text_tint`, cores e `frosted_windows`) aplicado com
  `cosmic::command::set_theme`: diálogo, dicas e gaveta seguem o tema. O `CosmicTheme` calcula o próprio tom do texto, então a cor
  exata da paleta vem do `Application::style` (cor padrão do texto) e dos estilos do app. Voltar a "sistema" usa
  `cosmic::theme::system_preference()`, que segue as Configurações.
- Terminais: `term::themes()` inclui a paleta ANSI de cada tema (`terminal_theme::from_palette`) e `Config::syntax_theme`
  escolhe pelo tema atual; na troca, `term::apply_theme` recolore os terminais abertos (chats e abas).
  Cores truecolor da TUI do `claude` não mudam, menos o fundo das mensagens do usuário (veja Terminal).
- **Desfoque do fundo**: item `[x] desfoque` no mesmo menu (`Message::ToggleBlur`), gravado em `$XDG_CONFIG_HOME/chube/blur`
  (`on`/`off`; sem o arquivo, ligado) e lido por `themes::blur()`. Desligado, `App::apply_blur` zera o `auto_blur` do `Core`
  (o libcosmic deixa de pedir o blur ao compositor e de usar as cores translúcidas do tema, mesmo com o "frosted" ligado) e
  `window_background` pinta as cores da paleta opacas (`Palette::solid`: a cor sobre o fundo do terminal do tema; só tirar o
  alfa deixaria o vidro branco). A troca reaplica o tema com `set_theme`, que é quando o libcosmic refaz o pedido.
- **Desfoque no GNOME** (o Mutter não atende o pedido do libcosmic): extensão própria em `gnome/desfoque-chube@pedro/`,
  instalada e ligada pelo `install.sh` (vale depois de relogar; também está no `aplicar.sh` do GNOME). Usa a mesma camada
  de vidro fosco da dock, do painel e dos menus (`desfoque.js`, cópia do da `temas-janelas@pedro`, que copia só as janelas
  de baixo): um filho do `MetaWindowActor` do Chube, por baixo do conteúdo, recortado com os cantos da janela.
  O filho é uma `BaseCamada`, que não se desenha dentro da cópia que outra camada faz da janela (desenhá-lo lá,
  com as janelas em monitores diferentes, estragava o vidro da janela de baixo; veja o `desfoque.js`).
  Quem decide é o Chube: `App::apply_blur` grava `~/.config/chube/desfoque` (`themes::save_blur_request`) com
  `on <raio> <brilho %> <cantos>` (o `Palette::blur` do tema e o raio dos cantos da janela) ou `off` (desfoque desligado ou
  tema opaco), e a extensão acompanha o arquivo. No tema do sistema, pede quando o fundo do COSMIC é translúcido.
- O fundo da janela tem os mesmos cantos que o libcosmic dá à janela (`window_corner_radius`: `radius_s` + 4, retos com
  `sharp_corners`); antes ele saía quadrado para fora da borda redonda.
- Testar a extensão sem relogar: um GNOME Shell `--headless --wayland-display=<nome>` com dconf, `XDG_DATA_HOME` e
  `XDG_CONFIG_HOME` próprios (exportados antes do `dbus-run-session`), a extensão ligada por link na pasta de extensões
  dele, uma extensão que liga o `unsafe_mode` (para o `Eval` e a captura pelo D-Bus) e o Chube aberto com
  `WAYLAND_DISPLAY=<nome>` e o mesmo `XDG_CONFIG_HOME`.
- Testar um tema sem mexer no escolhido: gravar o nome em `~/.config/chube/theme`, abrir com `XDG_STATE_HOME=<pasta>` e capturar a tela.

## Verificar a UI

O Claude **não consegue clicar nem digitar na janela** (o app não aparece no AT-SPI). Para validar:
- captura de tela: `cosmic-screenshot --interactive=false --modal=false --notify=false -s <pasta>`;
- fluxos de chat: abrir com `--model haiku --prompt "…"` e capturar a tela; status e sessão do `claude` do chat em
  `~/.claude/sessions/<pid>.json` (`pgrep -af "claude --session-id"`);
- para fechar o app: `pkill -x chube` (**não** `pkill -f …chube`, que casa com o próprio shell do comando).
  Se o Claude estiver rodando **dentro** de um Chube, abra uma segunda instância e feche só ela pelo pid;
- lógica fora da UI (parsers, resolução de diretório): testes em Rust;
- cliques e digitação (troca de chat, painel de sessões, diálogo de nova sessão, sub-chat, "Reabrir sessão",
  terminais, campo de mensagem, seletor de modo, `[term]`, atalhos de teclado, arrastar a janela pelos cabeçalhos, botões – □ ×): pedir para conferir manualmente;
- bytes que a TUI entende: script Python com `pty.fork()` rodando o `claude` (comandos locais como `/help` não gastam tokens).

## Status do projeto

Versão 0.1.0. O roteiro inicial de 5 passos está completo (2026-09-27): esqueleto, chat real, sessões salvas/retomar/
sub-chat/permissões, painel de consumo e terminal integrado. Em seguida (2026-09-27), o chat em stream-json com
markdown e cartão de permissão foi trocado pelo `claude` interativo num terminal por chat. Repositório git criado (branch `main`), **ainda sem commits**.
Instalado pelo `./install.sh` (2026-09-27): aparece no launcher como "Chube" e pode ser fixado no dock.

Validado por captura de tela: o `claude` do chat abre com `--prompt`, responde, e status, modelo e contexto aparecem.
Ainda não testado com clique: troca entre chats, painel de sessões, diálogo de nova sessão, sub-chat, "Reabrir sessão",
digitação nos terminais, fechar chat.

Limitações conhecidas:
- o status só vem de `~/.claude/sessions`, um arquivo interno do Claude Code (formato pode mudar);
- o terminal não tem menu de contexto, zoom, busca nem divisão em painéis (o cosmic-term tem);
- a leitura do `/usage` depende do texto do Claude Code.
