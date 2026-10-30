# aula-cli

CLI e interface gráfica nativa em Rust para AULA F75/F87/F99 no Linux, com seleção automática da interface HID.

```sh
cd ~/projetos/aula-cli
cargo build --release --offline
./target/release/aula-cli info --mode
./target/release/aula-cli info --battery
./target/release/aula-cli effects
./target/release/aula-cli set --color '#4cb5e8' --style static
./target/release/aula-cli set --color '#4cb5e8'
./target/release/aula-cli set --style waterfall
./target/release/aula-cli set --brightness-up
./target/release/aula-cli set --brightness-down
./target/release/aula-cli set --brightness 70
./target/release/aula-cli set --brightness +10
./target/release/aula-cli set --style breathing --dry-run
```

As opções omitidas mantêm os campos correspondentes. `--color` altera a cor do
estilo atual. Ao trocar entre estilos com paleta recuperada, `--style` transfere
a paleta ativa ao destino e mantém o brilho salvo no destino. Estilos internos
multicoloridos como waterfall não têm uma cor única configurável recuperada;
ao selecioná-los, prevalece seu comportamento próprio de cores. Combinar
`--style waterfall --color ...` retorna erro sem gravar. `effects` lista estilos.

O brilho recebe percentual ou variação percentual e respeita os níveis do
modelo (F75:0–9; F87/F99:0–4). Cada incremento não nulo avança ao menos um nível;
o valor é limitado aos extremos. `--dry-run` consulta e calcula sem gravar.
No F75 wireless, o campo de brilho foi corrigido por comparação com a rodinha
do teclado real; veja [a evidência](docs/brightness-evidence.md).
Antes de gravar, o modelo é confirmado por PSD; as alterações são confirmadas
por releitura. Uma atualização de paleta e efeito usa duas operações distintas;
uma falha entre elas pode deixar somente a paleta atualizada.

## Interface gráfica

A interface segue o visual do projeto `k617rgb`: tema escuro, lista de efeitos,
prévia animada do teclado, seletor de cor e controles de brilho.

```sh
cd ~/projetos/aula-cli
cargo run --release --offline --features gui --bin aula-gui
```

Para compilar e abrir o executável diretamente:

```sh
cargo build --release --offline --features gui --bins
./target/release/aula-gui
```

A compilação offline exige as dependências em cache; na primeira compilação em
outra máquina, remova `--offline` para baixá-las. A GUI usa OpenGL e suporta
Wayland e X11. O CLI continua disponível como `aula-cli` e não exige a feature.

- A abertura consulta modelo, conexão, efeito, brilho e bateria disponível.
  Use **Atualizar conexão** depois de conectar ou trocar o modo do teclado;
  isso também descarta ajustes pendentes e relê o estado.
- Escolha o efeito na coluna esquerda. Marque **Alterar cor** e/ou **Alterar
  brilho** para enviar esses campos; editar um controle também marca o campo.
  Desmarcados, os campos seguem as mesmas regras de preservação do CLI.
- **Aplicar ao teclado** é a única ação que grava configurações. A GUI compartilha
  o backend do CLI, incluindo identificação do modelo e confirmação por releitura.
  A comunicação roda em uma thread própria para manter a janela responsiva.
- **Favoritas** contém oito amostras editáveis: clique em **Editar**, escolha a
  amostra e altere a cor nos mesmos campos do painel, sem abrir outra janela.
  Use o seletor, hexadecimal ou campos RGB (0–255) e finalize com
  **Concluir**. Nas favoritas e em **Alterar cor**, o toggle **HEX / RGB**
  exibe somente os campos do formato escolhido; os dois começam em **HEX**.
  Trocar o formato preserva a cor selecionada. O botão direito em uma amostra também
  permite substituí-la pela cor selecionada. **Resetar** restaura as oito
  favoritas padrão e preserva o histórico. O campo HEX é compacto, com prefixo `#` visível. HEX e RGB mantêm a mesma altura;
  alternar o formato não redimensiona o painel nem a prévia do teclado.
- **Últimas usadas** guarda as oito cores distintas mais recentemente
  aplicadas com confirmação do teclado. Reutilizar uma cor a move para o início;
  ajustes só de brilho e gravações que falham não acrescentam cores. Clique numa
  amostra para selecioná-la e depois em **Aplicar ao teclado** para enviá-la.
  Favoritas e histórico são salvos automaticamente entre sessões da interface.
- Efeitos com cores internas ocultam a edição de cor; **Desligado** envia apenas
  o efeito. Não são oferecidos controles por tecla ou velocidade, cujos comandos
  não estão implementados no CLI.
- A prévia é ilustrativa, com layouts brasileiros ABNT2 de F75/F87/F99
  selecionáveis, incluindo Ç, AltGr e Enter em duas linhas. Setas, rodinha e
  indicador de conexão são desenhados sem depender de símbolos da fonte. A escolha do
  layout altera apenas o desenho, não o modelo detectado. A cor inicial da prévia
  é uma amostra, não uma leitura da paleta física do teclado.
- Bateria indisponível é exibida sem estimativas. Bluetooth pode ser detectado,
  mas não habilita a configuração RGB. Falhas e instruções de permissão aparecem
  na janela; as mesmas regras udev do CLI se aplicam.

## Modos e limitações reais

A detecção procura interfaces de cabo, receptor wireless e Bluetooth. Interfaces
do mesmo dispositivo físico são agrupadas. Quando há interface cabeada de
configuração, ela tem preferência sobre um receptor USB conectado e ocioso.
Vários teclados distintos causam um erro de ambiguidade; desconecte os adicionais.
Um receptor conectado, por si só, não prova que o teclado está ativo nele.

**Controle RGB:** o CLI seleciona dois backends para os mesmos modelos. Por cabo,
usa Feature Report6/520 (variante Fw26:Report9). Pelo receptor3554:fa09,
usa Input/OutputReport19/20 com consulta inicial de estado7, espera de conexão
quando necessária e identificaçãoPSD5. Depois usa comandos44/04 paraLED e49/09
para paleta. Bluetooth continua sem protocolo de configuração confirmado.

A conclusão anterior de que o dongle não permitia configuração vinha de um
recorte incompleto: o driver cria outro backend dinamicamente após a descoberta
wireless. Veja [docs/wireless-evidence.md](docs/wireless-evidence.md).

Para registrar somente comandos/respostas de fornecedor durante diagnóstico:

```sh
AULA_TRACE=1 aula-cli info --battery
```

O log não inclui relatórios comuns de teclas. Os testes automatizados usam
respostas simuladas; o funcionamento no teclado físico ainda requer validação.

**Bateria:** usa capacidade publicada pelo Linux para o próprio teclado ou a
consulta documentada pelo driver na interface de configuração compatível. Sem
leitura disponível, mostra `[??????????] indisponível`; nunca estima um valor.
Uma leitura real aparece como `[███████░░░] 70%`.

Se o teclado não for identificado como modelo conhecido, o CLI retorna erro.
Não há atualização de firmware nem macros nesta versão.

## Permissão Linux

`list` e `info --mode` usam sysfs. Consultas de configuração precisam de acesso
hidraw. Instale a regra se necessário e reconecte o teclado/receptor:

```sh
sudo install -m0644 udev/70-aula.rules /etc/udev/rules.d/70-aula.rules
sudo udevadm control --reload-rules
```

A regra atende sessões desktop locais systemd-logind; não é instalada pelo CLI.
Permissão de acesso não adiciona funcionalidades ao firmware.

## Desenvolvimento

CLI: Rust 1.88+, sem dependências externas ativas. Interface gráfica: Rust 1.95+
e eframe/egui 0.36, habilitados apenas com a feature `gui`. Módulos: cli (argumentos), device
(detecção/bateria sysfs), transport (HID), protocol (comandos), rgb (campos RGB).

```sh
cargo fmt --check
cargo clippy --offline --all-targets -- -D warnings
cargo test --offline
cargo clippy --offline --features gui --all-targets -- -D warnings
cargo test --offline --features gui
```

A engenharia reversa está resumida em `docs/`. Pseudocódigo e desmontagem são
evidências estáticas; testes com respostas simuladas não demonstram funcionamento
no teclado. A validação dos três modos exige alternar o teclado fisicamente.

Correção de fragmentação wireless: paletas usam o tamanho declarado pelo
receptor (a resposta física observada anunciou35 fragmentos), preservando os
bytes recebidos sem completar até512. Trocar para waterfall não consulta a
paleta, pois esse efeito não utiliza a paleta configurável recuperada.

Correção dos IDs de efeitos: o carregador do driver usa a PRIMEIRA coluna
LedOpt como ID do hardware (waterfall16, breathing2, rainbow3). A segunda
coluna é o tipo do efeito; não deve ser transmitida como ID.

Paleta do F75 wireless validada em captura: resposta completa de490 bytes
(35 fragmentos de14), com identificação do modelo no final e sem marcador.
A leitura verifica esse formato e preserva todos os bytes recebidos. Para
escrita, monta a imagemCRC=1 de512 bytes com marcador510/511, como no driver.
A releitura compara os dados recebidos novamente; não exige que o firmware
devolva o enquadramento acrescentado para escrita.
