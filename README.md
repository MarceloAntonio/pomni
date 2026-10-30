# Pomni

Controle de iluminação RGB para teclados no Linux, com interface gráfica nativa
e uma opção de uso pelo terminal.

Pomni é um projeto independente, sem afiliação, patrocínio ou aprovação da AULA.
As marcas citadas identificam os dispositivos compatíveis e pertencem aos seus
respectivos titulares.

## Recursos

- 19 efeitos de iluminação, com prévia animada do teclado.
- Seleção de cor por hexadecimal, RGB ou seletor visual.
- Controle de brilho, respeitando os níveis disponíveis no teclado.
- Oito cores favoritas editáveis, com opção de restaurar os padrões.
- Histórico das oito últimas cores usadas, salvo entre sessões.
- Exibição do modelo, conexão e bateria, quando disponível.
- Prévia de teclado brasileiro ABNT2, com Ç, AltGr e Enter em duas linhas.
- Interface para Wayland e X11, com comunicação em segundo plano.

## Compatibilidade

| Modelo | Cabo USB | Receptor 2,4 GHz | Bluetooth |
| --- | --- | --- | --- |
| AULA F75 | Implementado | Implementado; brilho validado no teclado real | Detecção; sem controle RGB |
| AULA F87 | Implementado | Implementado | Detecção; sem controle RGB |
| AULA F99 | Implementado | Implementado | Detecção; sem controle RGB |

O funcionamento depende da revisão do firmware. A validação física mais completa
foi feita em um F75 pelo receptor wireless; F87, F99 e outras conexões ainda
precisam de validação equivalente. A bateria aparece como indisponível quando o
teclado não informa o nível.

## Baixar e abrir

Baixe o pacote para Linux na página de
[releases](https://github.com/MarceloAntonio/pomni/releases), extraia e execute:

```sh
./pomni-gui
```

Os binários da release são para Linux x86_64 com glibc. Consulte os requisitos
específicos nas notas da versão. Para outras distribuições ou arquiteturas,
compile a partir do código-fonte.

### Permissão de acesso ao teclado

Se aparecer um erro de permissão, instale a regra udev incluída no pacote:

```sh
sudo install -m0644 udev/70-pomni.rules /etc/udev/rules.d/70-pomni.rules
sudo udevadm control --reload-rules
```

Reconecte o teclado ou receptor depois disso. A regra permite o acesso à sessão
desktop local; não é necessário executar o aplicativo como root.

## Usar a interface

1. Escolha um efeito na coluna da esquerda.
2. Se desejar mudar a cor ou o brilho, marque o campo correspondente ou edite seu
   controle. Campos desmarcados preservam a configuração existente.
3. Clique em **Aplicar ao teclado** para enviar e confirmar a configuração.

Use o toggle **HEX / RGB** para escolher o formato de entrada da cor. Os dois
editores começam em HEX. Alternar o formato mantém a cor e o tamanho do painel.

Em **Favoritas**, clique em **Editar**, selecione uma amostra e altere a cor nos
mesmos campos do painel. Finalize com **Concluir**. **Resetar** restaura as oito
favoritas padrão e preserva o histórico.

**Últimas usadas** mostra as oito cores distintas aplicadas com sucesso, da mais
recente para a mais antiga. Clique em uma amostra para selecioná-la e depois em
**Aplicar ao teclado** para enviá-la. Favoritas e histórico são salvos
automaticamente.

Use **Atualizar conexão** depois de reconectar ou trocar o modo do teclado.
Essa ação também descarta os ajustes pendentes e relê o estado. A escolha de
layout da prévia altera apenas o desenho; o modelo conectado é detectado
automaticamente.

Alguns efeitos usam cores próprias do firmware e não permitem selecionar uma
cor única. O projeto não oferece atualização de firmware, macros, remapeamento
ou iluminação por tecla.

## Terminal

O pacote também inclui o executável `pomni`:

```sh
./pomni info
./pomni info --battery
./pomni effects
./pomni set --style static --color '#4cb5e8' --brightness 70
./pomni set --brightness-up
./pomni set --brightness-down
./pomni set --style waterfall
./pomni set --brightness 50 --dry-run
```

Cor, efeito e brilho omitidos são preservados. Ao trocar entre efeitos
compatíveis, a paleta ativa acompanha a mudança. O brilho aceita percentual de
0 a 100 ou variação, como `--brightness +10`. O teclado trabalha com níveis
discretos, então o valor aplicado pode ser arredondado.

`--dry-run` consulta e calcula a alteração sem gravá-la. Para diagnóstico:

```sh
POMNI_TRACE=1 ./pomni info --battery
```

## Compilar

É necessário Rust 1.95+ para a GUI, além das ferramentas de compilação da sua
distribuição. O CLI isolado requer Rust 1.88+.

```sh
git clone https://github.com/MarceloAntonio/pomni.git
cd pomni
cargo build --release --locked --features gui --bins
./target/release/pomni-gui
```

Para compilar somente o CLI:

```sh
cargo build --release --locked --bin pomni
```

Verificações de desenvolvimento:

```sh
cargo fmt --check
cargo clippy --locked --features gui --all-targets -- -D warnings
cargo test --locked --features gui
```

As notas técnicas do protocolo e da validação estão em [docs](docs/).

## Licença

O código do Pomni é distribuído sob a [licença MIT](LICENSE). As dependências
mantêm suas respectivas licenças; os avisos aplicáveis acompanham os pacotes
binários. Este repositório não distribui o driver, firmware ou imagens
proprietárias do fabricante.
