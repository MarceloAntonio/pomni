# Seleção dos protocolos

O backend cabeado Fw24 usa Feature6/520 (varianteFw26:Feature9/520), payload
nooffset8: PSD82/página1, LED84/04, paleta8a/0a, bateria87/página2.

O driver também descobre o dongle pelo descritorFF02/2 Input/OutputReport19/20,
consulta estado7 e PSD5 e cria dinamicamente a família3632 para o mesmo modelo.
Portanto o formato report19 abaixo é usado em produção pelo caminho wireless.
A hipótese anterior de exigir cabo estava incompleta. Detalhes da análise em
`wireless-evidence.md`.

# Protocolo implementado

Artefato: `AULA F Series Driver Setup v2.0 20251127.exe`, SHA-256
`c2b3161f7da0a04c6f9d2efc6c7b124b29aa54618dbc84b2471c7bf7545d0952`.
Executável extraído `OemDrv.exe`, PE32 x86 nativo, SHA-256
`4091d39ef765b38c26fe77e3afb6f4378f52900edd3a144adb50c661238852ec`.

| Operação | Comando | Dados |
|---|---|---|
| PSD/modelo | 05 | 6 bytes |
| Ler RGB | 44 | 128 bytes |
| Gravar RGB | 04 | 128 bytes |
| Ler paleta | 49 | 512 bytes |
| Gravar paleta | 09 | 512 bytes |

Todos usam Output/Input report `13` hexadecimal (19 decimal), 20 bytes totais.
Cabeçalho: ID, comando, quantidade de pacotes, índice, camada/comprimento.
Payload tem até 14 bytes. Último byte transmitido: soma dos 19 anteriores,
módulo 256. Consultas enviam um pacote sem dados. Gravações são fragmentadas,
com ACK por índice e intervalo de 15 ms. O receptor mistura outros relatórios;
o CLI filtra pelo ID e pelo comando. O parser não presume que o checksum das
respostas segue a regra de transmissão, pois o original não o verifica nesse
caminho. Reordenação é tratada por índice; duplicatas contraditórias são erro.

Os perfis reconhecidos têm PSD `03 00 00 00 03 c1/c2/c3` (F87/F75/F99).
Nenhuma configuração é enviada para PSD desconhecido. A proteção não substitui
validação experimental da compatibilidade de revisões futuras de firmware.

RGB128: byte 9 seleciona modo normal (zero); byte 10 contém o ID de efeito.
Para índice de interface i, velocidade fica em `58+2*i`; brilho, no nibble alto
de `59+2*i`; aleatório, no nibble baixo. A paleta contém sete triplas RGB por
ID de efeito hardware. Para static (ID1), bytes 21–41. Os modelos analisados
declaram `CRC=1`: o software usa blocos com marcador final `5a a5`; isso não é
uma implementação de CRC matemático no CLI. Os marcadores existentes são
preservados e exigidos antes de uma alteração.

Evidência REA da decompilação focada: sessão
`ev_92b6296874220a4333f4d97d66f856f5088349428c6f2cef1e55d6a8724c5fee`;
96 funções exportadas. O pseudocódigo não é o código-fonte original. Os detalhes
de layout fora desse recorte foram obtidos por desmontagem, conforme endereços
nas notas; não recebem um ID REA inventado.

Imports REA: `ev_d0e6e0104fa82b0b7ad6541f58bb91fe1cdcf56e83fed0bebf0a7d93db6d4f64`.
As importações demonstram uso de APIs HID e SetupAPI, sem provar execução.
A coleta é estática; nenhum instalador/firmware foi executado e nenhuma sessão
Ghidra foi deixada aberta. Não há afirmação de recuperação integral do software.
