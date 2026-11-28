# claude-local.md

Regras de trabalho para o Claude Code (sessão principal e agentes) neste
repositório. Complementam o `AGENTS.md` e são importadas pelo `CLAUDE.md`.

## Verificar antes de afirmar

- Proibido fazer suposições sobre o código. Antes de definir que um código
  é morto, use o grep para buscar os usos reais.
- Verifique sempre o código; não faça apenas suposições sobre a semântica.
- Uma suposição sem código que a prove é tratada como
  **"Mentira - Supostamente"**. Ao relatar algo que não foi verificado,
  marque-o assim em vez de apresentá-lo como fato.
- Antes de declarar morto um campo ou uma função de struct (por exemplo,
  `Stage::shader`), verifique todas as ocorrências em cada alvo de
  configuração (`cfg`, features) e em cada módulo específico de
  plataforma. Não remova nem renomeie o elemento até concluir essa
  verificação, e inclua as evidências (comandos e resultados) no briefing
  do agente.

## Acompanhamento de PRs

- Assine a atividade de todos os PRs abertos e não consulte cada um
  individualmente.
- Quando chegarem notificações, responda com uma única tabela
  consolidada: `PR | issue | CI status | mergeable | next action`.
