# Brilho do F75 wireless: captura da rodinha e correção

## Evidência física de 2026-10-30

Teclado identificado por PSD `03 00 00 00 03 c2` (F75), receptor wireless,
Input/Output report 19. O CLI instalado foi usado com `AULA_TRACE=1` e
`set --brightness 70 --dry-run`: consulta de estado 07, PSD 05 e LED 44;
nenhum comando de gravação 04/09 foi enviado durante a comparação.

1. Leitura antes de girar a rodinha: efeito ativo LED[10] = 11 (`neon`).
2. O usuário diminuiu o brilho pela rodinha, mantendo o efeito.
3. Leitura depois: entre os 128 bytes LED, somente LED[78] mudou, de 6 para 2.
   LED[79] permaneceu 0. Bytes finais 126/127 permaneceram `5a a5`.

Os blocos completos decodificados das respostas foram preservados em
`tests/fixtures/f75-wireless-neon-before.bin` e
`tests/fixtures/f75-wireless-neon-after.bin`. Checksums e preenchimento dos
relatórios não fazem parte desses blocos de 128 bytes.

Para neon, índice UI i=10, o campo observado é `0x3a + 2*i = 78`.
A implementação anterior escrevia o nibble alto de `0x3b + 2*i = 79`:
a releitura confirmava a alteração desse outro campo, mas não demonstrava
alteração luminosa. A GUI também interpretava esse mesmo campo como brilho.

## Relação com a análise estática

O driver OemDrv.exe SHA-256
`4091d39ef765b38c26fe77e3afb6f4378f52900edd3a144adb50c661238852ec`
possui um conversor genérico em `0x49bc50`: no trecho `0x49bd40..0x49bd70`,
ele grava SpeedHW em `0x3a+2*i` e LightHW no nibble alto de `0x3b+2*i`.
Isso é observação estática do programa distribuído, não validação do formato
usado pela revisão de firmware do teclado atual.

Fonte da desmontagem preservada: `/home/celo/aula-re/evidence/oem-disassembly.txt`.
A análise REA anterior da família 3632 está preservada sob Evidence
`ev_92b6296874220a4333f4d97d66f856f5088349428c6f2cef1e55d6a8724c5fee`;
a comparação física e a desmontagem citada aqui não recebem um novo ID REA.

## Correção e limites

CLI e GUI selecionam `LedLayout::F75Wireless` após confirmar modelo e transporte.
Nesse caso, leem/escrevem o byte inteiro `0x3a+2*i`, com níveis 0..9.
O byte adjacente, incluindo o nibble baixo usado na seleção de cor, é preservado
quando só se altera brilho. A GUI usa a mesma leitura de brilho do backend.

A posição para os outros efeitos segue a tabela de pares por índice UI;
sua aplicação a todos os efeitos é uma inferência do layout. A comparação
física acima foi realizada no efeito neon. Não houve captura da rodinha por cabo
nem nos modelos F87/F99; esses caminhos mantêm o layout anterior do driver.

Os testes comparam o resultado de brilho absoluto e relativo com o bloco
independente capturado depois de girar a rodinha, e verificam preservação dos
demais campos. Testes simulados não substituem a confirmação visual do usuário.

## Validação da escrita corrigida

Depois de compilar CLI e GUI, executamos no teclado real
`./target/release/aula-cli set --brightness 67`. O CLI informou
`Brilho: 22% → 67%` e confirmou a configuração por releitura.
O usuário confirmou visualmente: “Sim, aumentou”. O efeito neon foi mantido.
Essa confirmação valida a escrita no F75 wireless observado; não amplia a
validação física para outros modelos, conexões ou efeitos. O brilho ficou
em 67%, correspondente ao nível 6 observado antes da comparação com a rodinha.
