# Desenvolvimento de aplicativo Windows para transcrição e assistência em reuniões

Atue como arquiteto de software e desenvolvedor sênior especializado em Rust, Tauri, React e aplicações de áudio para Windows.

Vamos criar um aplicativo desktop para Windows capaz de:

* Capturar simultaneamente o áudio do microfone do usuário e o áudio reproduzido pelo sistema.
* Transcrever localmente esses dois canais em tempo real.
* Organizar reuniões e transcrições por projetos e sessões.
* Gerar resumos, destaques, respostas e insights em tempo real usando modelos de linguagem acessados pelo OpenRouter.
* Manter os dados de áudio e transcrição sob controle do usuário, priorizando privacidade e processamento local.

Antes de implementar, analise as referências técnicas:

* Handy, como referência para gerenciamento e execução local de modelos de transcrição:
  https://github.com/cjpais/handy
* OpenRouter:
  https://openrouter.ai/docs/quickstart
* Tauri:
  https://github.com/tauri-apps/tauri
* shadcn/ui com Vite:
  https://ui.shadcn.com/docs/installation/vite

Não copie diretamente a arquitetura ou o código do Handy. Use-o apenas como referência técnica e de experiência do usuário, respeitando sua licença.

## 1. Objetivo do produto

O aplicativo será um assistente de reuniões em tempo real.

Durante uma sessão, ele deverá:

1. Capturar o áudio do microfone.
2. Capturar separadamente o áudio de saída do Windows.
3. Transcrever os dois fluxos localmente.
4. Exibir visualmente quem está falando com base na origem do áudio:

   * “Você”, para o microfone.
   * “Sistema” ou “Participante”, para o áudio de saída.
5. Enviar apenas o texto transcrito, e não o áudio, ao OpenRouter.
6. Gerar resumos parciais, possíveis respostas, pontos importantes, perguntas pendentes e outros insights.
7. Salvar todos os resultados localmente em arquivos Markdown.

O MVP será direcionado exclusivamente ao Windows. Não implemente suporte a macOS ou Linux nesta primeira versão, mas evite decisões arquiteturais que impeçam futura expansão.

## 2. Stack tecnológica obrigatória

Utilize:

* Tauri 2.
* Rust no backend.
* Vite.
* React.
* TypeScript com modo estrito.
* shadcn/ui.
* Tailwind CSS.
* pnpm.
* Zustand ou outra solução leve para estado global.
* TanStack Query somente quando trouxer benefícios reais para operações assíncronas.
* Zod para validação dos dados e configurações.
* OpenRouter para acesso aos modelos LLM.
* Whisper ou implementação compatível para transcrição local.
* Armazenamento local de configurações e metadados.
* Markdown com front matter YAML para documentos gerados.

Para inicializar o frontend, valide a sintaxe atual do shadcn antes de executar. A intenção inicial é usar um preset visual semelhante a:

```bash
pnpm dlx shadcn@latest init --preset b0 --template vite
```

Caso esse preset ou argumento não seja mais válido, utilize o comando equivalente atualmente recomendado e documente a alteração.

## 3. Princípios de arquitetura

Organize o projeto em módulos independentes:

* Captura de áudio.
* Processamento e normalização de áudio.
* Detecção de voz.
* Transcrição local.
* Gerenciamento e download de modelos.
* Gerenciamento de projetos e sessões.
* Persistência de documentos.
* Integração com OpenRouter.
* Construção de prompts.
* Geração de insights.
* Interface desktop e gerenciamento de janelas.
* Configurações e armazenamento seguro de credenciais.
* Métricas locais de uso.

O frontend não deve acessar diretamente arquivos sensíveis, credenciais ou APIs externas. Essas operações devem passar por comandos Rust do Tauri com permissões mínimas.

Defina contratos TypeScript e Rust claros para eventos como:

* `audio-level-updated`
* `transcription-partial`
* `transcription-final`
* `session-status-changed`
* `insight-generated`
* `summary-updated`
* `model-download-progress`
* `application-error`

Evite concentrar toda a lógica em um único arquivo, componente React ou comando Tauri.

## 4. Captura de áudio

O aplicativo deve detectar e listar:

* Microfones disponíveis.
* Dispositivos de saída disponíveis.
* Dispositivo padrão de entrada.
* Dispositivo padrão de saída.

A captura deverá manter dois canais lógicos independentes:

```text
microphone
system_output
```

Requisitos:

* Permitir selecionar o dispositivo de entrada.
* Permitir selecionar o dispositivo de saída que será monitorado.
* Exibir medidor de nível para cada canal.
* Permitir testar os dispositivos antes da sessão.
* Detectar silêncio e evitar transcrever trechos vazios.
* Manter timestamps consistentes entre os dois fluxos.
* Informar claramente quando um dispositivo for desconectado.
* Tentar recuperar a captura quando o dispositivo padrão mudar.
* Não gravar arquivos de áudio permanentemente por padrão.
* Oferecer uma configuração opcional para manter o áudio original da sessão.
* Documentar claramente qualquer limitação técnica da captura do áudio do sistema no Windows.

Não tente fazer diarização completa no MVP. A separação inicial dos interlocutores será baseada na origem do áudio.

## 5. Transcrição local

O usuário poderá baixar e gerenciar diferentes modelos locais de transcrição.

A tela de modelos deverá apresentar:

* Nome do modelo.
* Tamanho do download.
* Espaço ocupado em disco.
* Idiomas suportados.
* Desempenho estimado.
* Requisitos aproximados de memória.
* Status do download.
* Status de instalação.
* Possibilidade de excluir o modelo.
* Modelo selecionado como padrão.

Suporte inicialmente modelos Whisper compatíveis e projetados para execução local. A arquitetura deverá permitir adicionar outros mecanismos no futuro.

O usuário poderá escolher o modelo:

* Nas configurações globais.
* Ao criar ou iniciar uma sessão.

A transcrição deve possuir:

* Resultados parciais.
* Resultados finais.
* Identificação do canal de origem.
* Timestamp inicial e final.
* Idioma detectado ou configurado.
* Indicador de confiança quando disponível.
* Tratamento de falhas sem interromper toda a sessão.
* Fila de processamento com controle de pressão caso a transcrição fique mais lenta que o áudio recebido.

Considere aceleração de hardware quando disponível, mas mantenha fallback para CPU.

## 6. OpenRouter e modelos LLM

O OpenRouter será utilizado apenas para analisar texto.

A aplicação deverá permitir:

* Informar e validar uma API key.
* Armazenar a API key de forma segura no Windows.
* Listar ou pesquisar modelos disponíveis.
* Escolher modelos diferentes para:

  * Insights rápidos.
  * Resumo da sessão.
  * Perguntas manuais.
* Exibir informações relevantes, quando disponíveis:

  * Provedor.
  * Nome do modelo.
  * Context window.
  * Preço aproximado.
  * Suporte a streaming.
* Definir limite de gastos por sessão.
* Definir limite máximo de tokens.
* Utilizar streaming nas respostas em que ele melhorar a experiência.
* Cancelar requisições em andamento.
* Tratar timeout, rate limit, saldo insuficiente e falhas de provedor.
* Implementar retry limitado com backoff.
* Não registrar a API key em logs.
* Nunca salvar a API key em arquivos Markdown ou em texto simples.

Utilize a API compatível com OpenAI disponibilizada pelo OpenRouter.

A integração deverá permitir a troca de modelo sem alterações no restante da aplicação.

## 7. Projetos, sessões e contexto

Toda sessão deverá pertencer a um projeto.

### Projeto

Um projeto deverá possuir:

* ID.
* Nome.
* Descrição.
* Contexto global.
* Participantes opcionais.
* Tags.
* Data de criação.
* Data de atualização.
* Pasta local.
* Preset padrão.
* Modelo de transcrição padrão.
* Modelos LLM preferenciais.

Exemplos de projetos:

* Processo seletivo para engenheiro backend.
* Reuniões semanais da empresa.
* Descoberta de produto.
* Pesquisa acadêmica.
* Atendimento a cliente.

### Sessão

Antes de iniciar uma sessão, o usuário deverá definir:

* Título.
* Projeto.
* Objetivo.
* Contexto específico.
* Preset.
* Idioma.
* Dispositivos de áudio.
* Modelo local de transcrição.
* Modelo LLM para insights.
* Modelo LLM para resumo.
* Preferência sobre armazenamento do áudio.

O contexto final enviado ao LLM será formado por:

1. Contexto global do projeto.
2. Contexto específico da sessão.
3. Instruções do preset.
4. Trecho relevante da transcrição.
5. Histórico resumido da sessão.
6. Tipo de saída solicitado.

Não envie toda a transcrição novamente a cada chamada. Implemente uma estratégia de janela móvel, resumo acumulado e seleção de trechos relevantes para controlar custo e contexto.

## 8. Presets

Inclua inicialmente os seguintes presets:

* Entrevista técnica.
* Entrevista comportamental.
* Reunião de negócios.
* Reunião de vendas.
* Reunião de produto.
* Brainstorming.
* Aula ou palestra.
* Atendimento ao cliente.
* Preset personalizado.

Cada preset deve definir:

* Papel esperado do assistente.
* Objetivos da análise.
* Tipos de insights.
* Tom das respostas.
* Estrutura do resumo final.
* Itens que devem ser destacados.
* Itens que devem ser evitados.

Permita criar, editar, duplicar, exportar e excluir presets personalizados.

## 9. Interfaces principais

A aplicação terá três áreas principais, podendo utilizar múltiplas janelas nativas do Tauri quando apropriado.

### 9.1. Home e configurações

A Home deverá incluir:

* Lista de projetos.
* Sessões recentes.
* Botão para iniciar nova sessão.
* Acesso aos modelos locais.
* Configuração do OpenRouter.
* Configuração dos dispositivos de áudio.
* Configuração da pasta de armazenamento.
* Gerenciamento de presets.
* Preferências gerais.

O dashboard deverá apresentar dados locais como:

* Número de sessões.
* Tempo total transcrito.
* Projetos mais utilizados.
* Modelo Whisper mais utilizado.
* Modelos LLM mais utilizados.
* Tokens enviados e recebidos.
* Custo estimado por período.
* Quantidade de insights gerados.
* Espaço em disco utilizado.
* Erros recentes.

Não implemente telemetria externa no MVP. Essas métricas devem permanecer locais.

### 9.2. Janela de transcrição

A janela de transcrição deverá:

* Exibir a conversa em ordem cronológica.
* Diferenciar claramente microfone e saída do sistema.
* Mostrar timestamps.
* Destacar resultados ainda parciais.
* Substituir resultados parciais pela versão final.
* Fazer auto-scroll enquanto o usuário estiver no final.
* Interromper o auto-scroll quando o usuário navegar para mensagens anteriores.
* Oferecer um botão para retornar ao trecho mais recente.
* Permitir pausar e retomar a transcrição.
* Permitir inserir marcadores manualmente.
* Permitir editar trechos depois de finalizados.
* Permitir copiar trechos.
* Permitir pesquisar na sessão.
* Permitir perguntar ao LLM sobre um trecho específico.

Cada bloco de transcrição deverá possuir ações como:

* Perguntar sobre este trecho.
* Sugerir uma resposta.
* Explicar.
* Resumir.
* Marcar como importante.
* Copiar.
* Corrigir texto.

Ao perguntar sobre um trecho, envie ao LLM:

* O trecho selecionado.
* Algumas mensagens anteriores e posteriores.
* O contexto da sessão.
* O contexto resumido da conversa.

### 9.3. Janela de insights

A janela de insights deverá gerar recomendações com base nos trechos mais recentes.

Tipos iniciais:

* Sugestão do que responder.
* Perguntas de acompanhamento.
* Pontos que precisam ser esclarecidos.
* Fatos ou números mencionados.
* Riscos e objeções.
* Decisões tomadas.
* Tarefas e responsáveis.
* Contradições ou inconsistências.
* Tópicos ainda não abordados.

A interface deverá:

* Exibir um insight por vez ou em cartões.
* Permitir navegar entre insights anteriores e seguintes.
* Permitir fixar um insight.
* Permitir descartar.
* Permitir copiar.
* Permitir gerar uma alternativa.
* Informar a qual trecho da transcrição o insight está relacionado.
* Diferenciar insights provisórios de insights confirmados.
* Evitar gerar repetidamente o mesmo insight.

Os insights devem ser úteis, objetivos e breves. Não devem interromper o usuário com atualizações irrelevantes a cada frase.

## 10. Controle das janelas

As janelas de transcrição e insights deverão possuir:

* Controle independente de opacidade.
* Opção “sempre visível”.
* Redimensionamento.
* Posição persistida.
* Tamanho persistido.
* Modo compacto.
* Possibilidade de ocultar rapidamente.
* Atalho configurável para mostrar ou ocultar.
* Opção de bloquear interação do mouse, caso seja tecnicamente viável e segura.
* Preferência de monitor em configurações com múltiplas telas.

A opacidade deve afetar o fundo da janela sem comprometer excessivamente a legibilidade do texto.

## 11. Persistência em Markdown

Todos os documentos gerados deverão ser armazenados localmente em Markdown.

Estrutura sugerida:

```text
workspace/
  projects/
    <project-slug>/
      project.md
      presets/
      sessions/
        <yyyy-mm-dd-session-slug>/
          session.md
          transcript.md
          summary.md
          insights.md
          actions.md
          questions.md
          audio/
```

Cada documento deverá possuir front matter YAML.

Exemplo para `transcript.md`:

```yaml
---
schema_version: 1
document_type: transcript
project_id: "project-uuid"
session_id: "session-uuid"
title: "Reunião semanal de produto"
created_at: "2026-08-05T14:00:00Z"
updated_at: "2026-08-05T15:20:00Z"
language: "pt-BR"
transcription_engine: "whisper"
transcription_model: "model-id"
microphone_device: "device-id"
output_device: "device-id"
participants:
  - "Usuário"
tags:
  - produto
  - planejamento
---
```

Cada entrada da transcrição deverá preservar:

* ID.
* Canal.
* Timestamp.
* Texto.
* Status parcial ou final.
* Data da última edição.
* Referência opcional ao arquivo de áudio.

Exemplo:

```markdown
## 00:03:14 — Participante

Precisamos finalizar a primeira versão até sexta-feira.

<!--
segment_id: segment-uuid
source: system_output
start_ms: 194000
end_ms: 198500
status: final
-->
```

O `summary.md` deverá conter:

* Resumo executivo.
* Principais temas.
* Decisões.
* Tarefas.
* Responsáveis.
* Prazos.
* Riscos.
* Perguntas em aberto.
* Próximos passos.

O salvamento deverá ser incremental e resiliente. Uma falha ou encerramento inesperado não deve apagar a sessão inteira.

Utilize gravação atômica, arquivos temporários ou uma estratégia equivalente para reduzir risco de corrupção.

## 12. Banco de dados e índice local

Os documentos Markdown serão a fonte de dados portável e legível pelo usuário.

Entretanto, poderá ser utilizado um banco local leve, como SQLite, para:

* Indexação.
* Pesquisa.
* Relacionamento entre projetos e sessões.
* Configurações.
* Cache.
* Estado de downloads.
* Métricas.
* Recuperação rápida da interface.

O banco não deverá ser a única cópia das transcrições e documentos importantes.

Documente claramente quais dados ficam no banco e quais ficam nos arquivos Markdown.

## 13. Segurança e privacidade

Requisitos obrigatórios:

* Processar áudio e transcrição localmente.
* Enviar ao OpenRouter apenas os textos necessários.
* Exibir claramente quando o conteúdo estiver sendo enviado a um serviço externo.
* Permitir desativar completamente os recursos de LLM.
* Armazenar a API key utilizando mecanismo seguro do sistema operacional.
* Aplicar permissões mínimas no Tauri.
* Validar todos os caminhos de arquivos.
* Impedir path traversal.
* Nunca executar conteúdo vindo da transcrição.
* Não renderizar Markdown não confiável sem sanitização.
* Não incluir segredos em logs, relatórios ou mensagens de erro.
* Permitir excluir definitivamente projetos e sessões.
* Permitir exportar os dados sem dependência do aplicativo.
* Não incluir telemetria externa sem consentimento explícito.

Inclua um aviso de que o usuário é responsável por observar as leis e regras de consentimento aplicáveis à gravação e transcrição de reuniões.

## 14. Estados e tratamento de erros

Modele explicitamente os estados da sessão:

```text
idle
preparing
capturing
transcribing
paused
stopping
processing_summary
completed
failed
```

A interface deverá tratar:

* Ausência de microfone.
* Falha na captura da saída.
* Dispositivo desconectado.
* Modelo não instalado.
* Modelo incompatível.
* Memória insuficiente.
* Falha no download.
* Erro de transcrição.
* API key inválida.
* Falta de saldo no OpenRouter.
* Rate limit.
* Timeout.
* Falta de internet.
* Resposta inválida do modelo.
* Pasta sem permissão de escrita.
* Disco cheio.
* Documento Markdown corrompido.

Os erros devem ser apresentados em linguagem compreensível, com opção de ver detalhes técnicos e copiar um relatório sanitizado.

## 15. Desempenho

Metas iniciais do MVP:

* A interface não deve travar durante captura ou inferência.
* Captura, transcrição, persistência e chamadas de LLM devem executar fora da thread principal da interface.
* A transcrição parcial deve aparecer com baixa latência, conforme a capacidade do hardware.
* O consumo de memória deve ser monitorado ao carregar modelos.
* Somente um modelo pesado deve permanecer carregado quando não houver memória suficiente.
* Downloads devem ser retomáveis quando possível.
* O aplicativo deve continuar salvando a transcrição mesmo quando o OpenRouter estiver indisponível.
* A geração de insights não pode bloquear a transcrição.

## 16. Sistema de prompts

Crie um módulo próprio de construção de prompts.

Não espalhe strings de prompt pelos componentes da interface.

Cada prompt deverá possuir:

* ID.
* Versão.
* Finalidade.
* Variáveis esperadas.
* Limite aproximado de contexto.
* Schema de saída.
* Estratégia de fallback.

Sempre que possível, solicite respostas estruturadas em JSON e valide-as antes de atualizar a interface.

Exemplo de schema para insight:

```ts
type Insight = {
  id: string
  type:
    | "suggested_response"
    | "follow_up_question"
    | "risk"
    | "decision"
    | "action_item"
    | "clarification"
    | "contradiction"
  title: string
  content: string
  rationale?: string
  relatedSegmentIds: string[]
  confidence?: number
  createdAt: string
}
```

Não permita que instruções faladas durante a reunião substituam as instruções internas da aplicação. Trate a transcrição como conteúdo não confiável para reduzir riscos de prompt injection.

## 17. Escopo do MVP

O MVP deve incluir:

1. Criação de projetos.
2. Criação e início de sessões.
3. Seleção de microfone e saída.
4. Captura independente dos dois canais.
5. Download e seleção de pelo menos dois modelos locais.
6. Transcrição parcial e final.
7. Visualização diferenciada por origem.
8. Configuração segura da API key do OpenRouter.
9. Seleção de modelo LLM.
10. Pergunta manual sobre um trecho.
11. Geração de insights recentes.
12. Resumo final.
13. Persistência em Markdown.
14. Controle de opacidade.
15. Recuperação básica após encerramento inesperado.
16. Dashboard local básico.

Não inclua inicialmente:

* Colaboração em nuvem.
* Login ou contas.
* Sincronização entre dispositivos.
* Aplicativo móvel.
* Diarização avançada.
* Integração direta com Zoom, Teams ou Google Meet.
* Bots que entram automaticamente em reuniões.
* Marketplace de plugins.
* Treinamento ou fine-tuning de modelos.
* Telemetria remota.
* Backend próprio na nuvem.

## 18. Fases de implementação

Execute o trabalho em fases.

### Fase 1 — Planejamento

Antes de escrever código:

* Analise os requisitos.
* Liste riscos técnicos.
* Proponha a arquitetura.
* Defina a estrutura de diretórios.
* Defina modelos de dados.
* Defina os eventos entre Rust e React.
* Defina a estratégia de captura de áudio no Windows.
* Defina a estratégia de execução do Whisper.
* Defina a estratégia de persistência.
* Identifique dependências e respectivas licenças.
* Separe decisões confirmadas de hipóteses.

### Fase 2 — Fundação

* Inicialize Tauri, Vite, React e TypeScript.
* Configure shadcn/ui.
* Configure lint, formatação e testes.
* Crie navegação e layout.
* Crie gerenciamento de estado.
* Implemente armazenamento de configurações.
* Implemente logging sanitizado.
* Configure capabilities e permissões mínimas.

### Fase 3 — Áudio e transcrição

* Liste dispositivos.
* Implemente testes de entrada e saída.
* Capture os dois canais.
* Crie pipeline de áudio.
* Integre o modelo local.
* Implemente resultados parciais e finais.
* Exiba a transcrição em tempo real.

### Fase 4 — Projetos e persistência

* Crie projetos e sessões.
* Implemente estrutura de pastas.
* Implemente front matter.
* Implemente salvamento incremental.
* Implemente recuperação de sessão.
* Implemente pesquisa local básica.

### Fase 5 — OpenRouter e insights

* Implemente armazenamento seguro da chave.
* Implemente seleção de modelos.
* Implemente prompts versionados.
* Implemente perguntas sobre trechos.
* Implemente insights.
* Implemente resumo acumulado e resumo final.
* Implemente controle de custos e tokens.

### Fase 6 — Experiência desktop

* Implemente múltiplas janelas.
* Implemente opacidade.
* Implemente “sempre visível”.
* Implemente modo compacto.
* Implemente atalhos.
* Persista posições e dimensões.

### Fase 7 — Qualidade

* Testes unitários.
* Testes de integração.
* Testes do pipeline de áudio.
* Testes de persistência.
* Testes de recuperação.
* Testes com internet indisponível.
* Testes com diferentes dispositivos de áudio.
* Testes com diferentes capacidades de hardware.
* Testes de segurança das permissões Tauri.

## 19. Critérios de aceitação do MVP

O MVP será considerado funcional quando:

* O usuário conseguir criar um projeto e uma sessão.
* O aplicativo detectar microfone e dispositivo de saída.
* Os dois canais forem capturados separadamente.
* A fala aparecer na interface com origem e timestamp.
* O usuário conseguir baixar e selecionar um modelo local.
* A transcrição continuar funcionando sem internet.
* O usuário conseguir configurar o OpenRouter e selecionar um modelo.
* Uma pergunta sobre um trecho retornar uma resposta contextualizada.
* Insights forem gerados sem bloquear a transcrição.
* Ao encerrar a sessão, os arquivos Markdown forem gerados.
* O resumo possuir decisões, tarefas e perguntas em aberto.
* O aplicativo recuperar uma sessão interrompida sem perder toda a transcrição.
* A API key não aparecer em arquivos, logs ou mensagens de erro.
* A opacidade das janelas de transcrição e insights puder ser controlada separadamente.
* A indisponibilidade do OpenRouter não impedir a captura e a transcrição local.

## 20. Forma esperada das respostas durante o desenvolvimento

Ao trabalhar neste projeto:

1. Não implemente toda a aplicação de uma vez.
2. Comece apresentando a arquitetura e o plano de execução.
3. Explique decisões técnicas importantes.
4. Informe os arquivos que serão criados ou alterados.
5. Gere código completo, tipado e executável.
6. Evite pseudocódigo quando a implementação real for possível.
7. Não invente APIs de bibliotecas.
8. Valide APIs e versões na documentação oficial.
9. Não esconda limitações técnicas.
10. Execute ou descreva testes verificáveis para cada etapa.
11. Pare ao final de cada fase com:

    * O que foi concluído.
    * Como testar.
    * Limitações conhecidas.
    * Próxima etapa recomendada.

Comece agora pela Fase 1.

Entregue:

* Resumo do entendimento do produto.
* Arquitetura proposta.
* Diagrama textual dos componentes.
* Fluxo de dados da captura até o Markdown.
* Estrutura de diretórios.
* Modelos de dados principais.
* Eventos e comandos Tauri.
* Dependências sugeridas.
* Riscos técnicos.
* Decisões que precisam ser validadas por um protótipo.
* Plano incremental de implementação.
