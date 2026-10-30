# Campos RGB recuperados (análise estática)

Artefato: OemDrv.exe SHA-256 `4091d39ef765b38c26fe77e3afb6f4378f52900edd3a144adb50c661238852ec`.
Fonte: `evidence/oem-disassembly.txt`, GNU objdump; os endereços abaixo são VAs de análise. Estas notas complementares não são Evidence IDs REA. Não executamos o programa nem enviamos comandos ao hardware nesta análise.

## Prova da família

Vtable CDev3632 em `0x62220c`: slot `0x3c` aponta para ApplySetting `0x495190`, que chama código comum `0x49c080` em `0x4952a5`; este chama conversão `0x49bc50` em `0x49cb86`. Slots `0x6c/0x70` apontam SetLED/GetLED `0x4945f0/0x494640`; slots `0x7c/0x80` apontam SetEData/GetEData `0x494780/0x4947c0`. Portanto os campos abaixo seguem a execução estática da família correta, apesar do código de conversão comum ter nomes genéricos.

## Bloco LED de 128 bytes

**Correção por evidência física:** no F75 wireless observado em 2026-10-30,
o brilho está no byte `0x3a+2*i`, diferente do conversor genérico abaixo.
CLI e GUI usam o layout observado nesse modelo/transporte. Veja
[brightness-evidence.md](brightness-evidence.md) para as capturas e os limites.


* `[9]` seleciona estado de iluminação personalizada: a rotina de efeito `0x4950e0` força zero antes da gravação.
* `[10]` é o ID hardware do efeito, primeira coluna de `LedOptN` (segunda coluna é tipo/efeito). Escrita em `0x49bd0a` e `0x495153`.
* Para índice UI `i = N-1` de `LedOptN`, `[0x3a+2*i]` armazena `SpeedHW[speed_UI]`.
* `[0x3b+2*i] = (LightHW[brightness_UI] << 4) | (random_enabled ? RandomVal : 0)`.
* Evidência do laço: `0x49bd40..0x49bd70`, início destino `buf+0x3b`, incremento2,19 elementos. Fontes SpeedHW/LightHW são tabelas modelo+`0x2e10/0x2df0`, respectivamente.
* RandomVal tem default7: loader `0x41228a..0x4122ac`, chave UTF16 `RandomVal` VA`0x60f8a8`; nenhum KB.ini desses três modelos o altera. O nibble baixo NÃO é direção; é flag/valor de aleatoriedade.
* Modelo F75 LightHW é0..9; F87/F99 LightHW0..4. SpeedHW0..4 em todos.
* Quando CRC=1, `[126:128] = 5a a5`; não é soma CRC calculada. Leitura valida marcador em `0x49cb05..0x49cb17`, escrita em `0x49c02b..0x49c02f`.

## Paleta RGB separada, 512 bytes para estes modelos

SetEData wrapper `0x494780` envia comando `0x09`, camada0, através AccessData; GetEData `0x4947c0` solicita `0x49`, camada0, via SendCMD.

Rotina `0x49c080`, trecho `0x49c810..0x49c96b`, itera19 LedOpt, usa hardware effect ID `h` e base `h*21`, gravando sete triplets RGB em offsets0,3,6,9,12,15,18 relativos a essa base. RGBIndex=0,1,2 nos KB.ini estabelece ordem R,G,B; fontes são COLORREF low8/high8/high16.

A rotina monta420 bytes (20 paletas*21), porém quando CRC=1 copia estes420 para buffer512 e marca `[510:512] = 5a a5`, depois envia512. Evidência: `0x49c9ab..0x49ca33`. O flag é modelo+`0x2f7d`, carregado da chave CRC@`0x60f8f4` e armazenado em `0x412302`. Os três modelos declaram CRC=1, portanto NÃO usar420 na transação destes modelos.

Para cor fixa de efeito hardware1 (primeiro UI), recuperar512 bytes com0x49, substituir os sete triplets21..41 pela cor escolhida, preservar bytes restantes, gravar0x09, e limpar nibble random da entrada correspondente em LED128. Outros efeitos com hardware ID>=20 têm base>=420, fora da região significativa enviada pelo programa nessa variante; esta observação impede afirmar arbitrariamente que suas paletas sejam programáveis por esse caminho.

O caminho original contém esperas antes de enviar paleta e antes de gravar LED; aguardar ACK e manter intervalo é prudente. Não foi observado/validado efeito físico nem persistência em flash.
