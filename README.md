# aula-cli

CLI Rust Linux para AULA F75/F87/F99, com seleção automática da interface HID.

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
Antes de gravar, o modelo é confirmado por PSD; as alterações são confirmadas
por releitura. Uma atualização de paleta e efeito usa duas operações distintas;
uma falha entre elas pode deixar somente a paleta atualizada.

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
Não há atualização de firmware, macros, nem interface gráfica nesta versão.
O projeto gráfico indicado pelo usuário será referência após validação do CLI.

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

Rust1.88+, sem dependências externas. Módulos: cli (argumentos), device
(detecção/bateria sysfs), transport (HID), protocol (comandos), rgb (campos RGB).

```sh
cargo fmt --check
cargo clippy --offline --all-targets -- -D warnings
cargo test --offline
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
