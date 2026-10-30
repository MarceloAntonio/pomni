# Semântica de ajustes pelo CLI

`set --color RRGGBB` mantém o estilo atual. `set --style NOME` copia os sete elementos RGB da paleta ativa e a seleção de cores aleatórias para o novo estilo quando ambos possuem paleta configurável recuperada. O brilho salvo do estilo escolhido é preservado. Os demais estilos mantêm suas paletas e comportamento próprios: estilos multicoloridos, efeitos sem suporte a cor na coluna final de LedOpt ou a saída de um modo personalizado não aceitam garantir a manutenção de uma cor única. A camada de execução informa essa limitação quando aplicável.

`set --style breathing --color ff0000` escolhe o estilo e define seus sete elementos de paleta como vermelho. As cores só são programadas para efeitos cuja paleta foi recuperada e cuja configuração original admite cor. Efeitos com ID de hardware >=20 e os estilos rainbow/bloom rejeitam `--color` sem alterar buffers. A seleção desses estilos sem `--color` continua disponível.

`set --brightness 50` aproxima50% ao nível de hardware mais próximo. Há10 níveis no F75 e5 no F87/F99. `--brightness +10`/`-10` e `--brightness-up`/`--brightness-down` avançam ou recuam o equivalente a10 pontos, arredondando para pelo menos um nível quando o ajuste é diferente de zero. Os limites são o mínimo e o máximo de hardware. A alteração de brilho preserva a seleção de cores aleatórias.

`--color '#ff0000'` precisa de aspas porque o shell interpreta um `#` não protegido como início de comentário. `--color ff0000` também funciona.

`info`, `info --battery`, `info --mode` e a combinação dos seletores usam detecção automática. A camada de execução deve declarar quando uma informação não foi recuperada ou não está disponível; o parser não inventa valores de bateria nem de conexão.

Os testes de parser cobrem opções independentes, cores com/sem prefixo, estilos desconhecidos, duplicatas e faixas. Os testes de RGB usam blocos simulados com marcadores5aa5 e verificam transferência de cores entre estilos compatíveis, preservação dos bytes alheios ao ajuste e ausência de mudanças em entradas inválidas. Não representam uma captura de execução física do teclado.

No F75 wireless, leitura e escrita de brilho usam o byte `58+2*i`, identificado
por comparação antes/depois de girar a rodinha no teclado real. O layout anterior
alterava o byte adjacente e podia confirmar a gravação sem alterar o brilho.
Nos demais modelos/conexões, o layout do driver permanece inalterado por falta
de captura física correspondente. Evidência em `brightness-evidence.md`.
