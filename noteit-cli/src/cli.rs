use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[command(
    name = "noteit",
    version,
    about = "Note-it — Linha de comando",
    long_about = "Note-it CLI\n\nInterface de linha de comando headless para o Note-it.",
    disable_help_subcommand = true
)]
pub struct CliArgs {
    /// Emitir um único documento JSON em vez de texto para pessoas
    ///
    /// Global on purpose: it is accepted before the command and after it, and
    /// at every level of a grouped command, so no consumer has to remember
    /// where the option goes. It is an option and not a word — after the `--`
    /// escape, `--json` is payload like any other text.
    #[arg(long = "json", global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<CliCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum TaskStateArg {
    #[default]
    #[value(name = "pendentes", alias = "pending")]
    Pendentes,
    #[value(name = "concluidas", alias = "completed")]
    Concluidas,
    #[value(name = "todas", alias = "all")]
    Todas,
}

impl From<TaskStateArg> for noteit_core::TaskStateFilter {
    fn from(arg: TaskStateArg) -> Self {
        match arg {
            TaskStateArg::Pendentes => noteit_core::TaskStateFilter::Pending,
            TaskStateArg::Concluidas => noteit_core::TaskStateFilter::Completed,
            TaskStateArg::Todas => noteit_core::TaskStateFilter::All,
        }
    }
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    /// Mostrar ajuda sobre os comandos
    #[command(name = "ajuda", alias = "help")]
    Ajuda,

    /// Mostrar a versão do Note-it
    #[command(name = "versao", alias = "version")]
    Versao,

    /// Verificar o ambiente e diretórios do Note-it
    #[command(name = "status")]
    Status,

    /// Listar notas vivas em ordem de atualização
    #[command(name = "listar", alias = "list")]
    Listar {
        /// Limite de notas exibidas (1 a 100)
        #[arg(long = "limite", alias = "limit")]
        limite: Option<usize>,

        /// Filtrar por tag (repetível para múltiplos filtros com AND)
        #[arg(long = "tag", action = clap::ArgAction::Append)]
        tag: Vec<String>,

        /// Filtrar por propriedade chave=valor (repetível com AND)
        #[arg(long = "propriedade", alias = "property", action = clap::ArgAction::Append)]
        propriedade: Vec<String>,
    },

    /// Ler uma nota pelo UUID ou prefixo hexadecimal
    #[command(name = "ler", alias = "read")]
    Ler {
        /// UUID completo ou prefixo de no mínimo 8 caracteres hexadecimais
        id: String,
    },

    /// Buscar notas pelo corpo de texto
    #[command(name = "buscar", alias = "search")]
    Buscar {
        /// Termo de busca textual (case e accent-insensitive)
        consulta: String,

        /// Limite de notas exibidas (1 a 100)
        #[arg(long = "limite", alias = "limit")]
        limite: Option<usize>,

        /// Filtrar por tag (repetível para múltiplos filtros com AND)
        #[arg(long = "tag", action = clap::ArgAction::Append)]
        tag: Vec<String>,

        /// Filtrar por propriedade chave=valor (repetível com AND)
        #[arg(long = "propriedade", alias = "property", action = clap::ArgAction::Append)]
        propriedade: Vec<String>,
    },

    /// Listar catálogo de tags derivadas, ou alterar as tags de uma nota
    #[command(name = "tags")]
    Tags {
        #[command(subcommand)]
        command: Option<TagsCommand>,
    },

    /// Listar catálogo de propriedades, ou alterar as de uma nota
    #[command(name = "propriedades", alias = "properties")]
    Propriedades {
        #[command(subcommand)]
        command: Option<PropertiesCommand>,
    },

    /// Listar tarefas das notas
    #[command(name = "tarefas", alias = "tasks")]
    Tarefas {
        /// Estado das tarefas: pendentes, concluidas ou todas
        #[arg(long = "estado", alias = "state", default_value = "pendentes")]
        estado: TaskStateArg,

        /// Limite de tarefas exibidas (1 a 100)
        #[arg(long = "limite", alias = "limit")]
        limite: Option<usize>,

        /// Filtrar notas de origem por tag (repetível com AND)
        #[arg(long = "tag", action = clap::ArgAction::Append)]
        tag: Vec<String>,

        /// Filtrar notas de origem por propriedade chave=valor (repetível com AND)
        #[arg(long = "propriedade", alias = "property", action = clap::ArgAction::Append)]
        propriedade: Vec<String>,

        #[command(subcommand)]
        command: Option<TasksCommand>,
    },

    /// Listar notas na lixeira, ou restaurar uma nota dela
    #[command(name = "lixeira", alias = "trash")]
    Lixeira {
        #[command(subcommand)]
        command: Option<TrashCommand>,
    },

    /// Criar uma nota nova e devolver o identificador dela
    ///
    /// A nota é criada no repositório e nada é aberto: nenhuma janela, nenhum
    /// foco, nenhuma mudança de estado. O comportamento é o mesmo com o
    /// Note-it aberto ou fechado.
    #[command(name = "criar", alias = "create")]
    Criar {
        /// Markdown inicial da nota
        texto: Option<String>,

        /// Ler o Markdown inicial da entrada padrão
        #[arg(long = "stdin")]
        stdin: bool,

        /// Tag a aplicar já na criação (repetível)
        #[arg(long = "tag", action = clap::ArgAction::Append)]
        tag: Vec<String>,

        /// Propriedade chave=valor a aplicar já na criação (repetível)
        #[arg(long = "propriedade", alias = "property", action = clap::ArgAction::Append)]
        propriedade: Vec<String>,
    },

    /// Acrescentar Markdown ao final de uma nota
    #[command(name = "adicionar", alias = "append")]
    Adicionar {
        /// UUID completo ou prefixo de no mínimo 8 caracteres hexadecimais
        id: String,

        /// Markdown a acrescentar
        texto: Option<String>,

        /// Ler o Markdown a acrescentar da entrada padrão
        #[arg(long = "stdin")]
        stdin: bool,
        /// Só gravar se a nota ainda estiver nesta revisão
        ///
        /// A revisão vem do campo `revision` de `noteit ler --json`. Sem ela a
        /// gravação é incondicional, como sempre foi.
        #[arg(long = "if-revision", value_name = "REVISAO")]
        if_revision: Option<String>,
    },

    /// Substituir todo o corpo Markdown de uma nota
    ///
    /// Não abre um editor: o novo corpo vem do argumento ou da entrada padrão.
    #[command(name = "editar", alias = "edit")]
    Editar {
        /// UUID completo ou prefixo de no mínimo 8 caracteres hexadecimais
        id: String,

        /// Novo corpo Markdown da nota
        texto: Option<String>,

        /// Ler o novo corpo da entrada padrão
        #[arg(long = "stdin")]
        stdin: bool,

        /// Esvaziar o corpo da nota, declarando a intenção explicitamente
        #[arg(long = "vazio", alias = "empty")]
        vazio: bool,
        /// Só gravar se a nota ainda estiver nesta revisão
        ///
        /// A revisão vem do campo `revision` de `noteit ler --json`. Sem ela a
        /// gravação é incondicional, como sempre foi.
        #[arg(long = "if-revision", value_name = "REVISAO")]
        if_revision: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum TagsCommand {
    /// Adicionar uma tag a uma nota
    #[command(name = "adicionar", alias = "add")]
    Adicionar {
        /// UUID completo ou prefixo de no mínimo 8 caracteres hexadecimais
        id: String,
        /// A tag a adicionar
        tag: String,
        /// Só gravar se a nota ainda estiver nesta revisão
        ///
        /// A revisão vem do campo `revision` de `noteit ler --json`. Sem ela a
        /// gravação é incondicional, como sempre foi.
        #[arg(long = "if-revision", value_name = "REVISAO")]
        if_revision: Option<String>,
    },

    /// Remover uma tag de uma nota
    #[command(name = "remover", alias = "remove")]
    Remover {
        /// UUID completo ou prefixo de no mínimo 8 caracteres hexadecimais
        id: String,
        /// A tag a remover
        tag: String,
        /// Só gravar se a nota ainda estiver nesta revisão
        ///
        /// A revisão vem do campo `revision` de `noteit ler --json`. Sem ela a
        /// gravação é incondicional, como sempre foi.
        #[arg(long = "if-revision", value_name = "REVISAO")]
        if_revision: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum PropertiesCommand {
    /// Definir uma propriedade de uma nota
    #[command(name = "definir", alias = "set")]
    Definir {
        /// UUID completo ou prefixo de no mínimo 8 caracteres hexadecimais
        id: String,
        /// A propriedade no formato chave=valor
        atribuicao: String,
        /// Só gravar se a nota ainda estiver nesta revisão
        ///
        /// A revisão vem do campo `revision` de `noteit ler --json`. Sem ela a
        /// gravação é incondicional, como sempre foi.
        #[arg(long = "if-revision", value_name = "REVISAO")]
        if_revision: Option<String>,
    },

    /// Remover uma propriedade de uma nota
    #[command(name = "remover", alias = "remove")]
    Remover {
        /// UUID completo ou prefixo de no mínimo 8 caracteres hexadecimais
        id: String,
        /// A chave da propriedade a remover
        chave: String,
        /// Só gravar se a nota ainda estiver nesta revisão
        ///
        /// A revisão vem do campo `revision` de `noteit ler --json`. Sem ela a
        /// gravação é incondicional, como sempre foi.
        #[arg(long = "if-revision", value_name = "REVISAO")]
        if_revision: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum TasksCommand {
    /// Concluir uma tarefa
    #[command(name = "concluir", alias = "complete")]
    Concluir {
        /// UUID completo ou prefixo de no mínimo 8 caracteres hexadecimais
        id: String,
        /// A referência de 8 caracteres mostrada por `noteit tarefas`
        referencia: String,
        /// Só gravar se a nota ainda estiver nesta revisão
        ///
        /// A revisão vem do campo `revision` de `noteit ler --json`. Sem ela a
        /// gravação é incondicional, como sempre foi.
        #[arg(long = "if-revision", value_name = "REVISAO")]
        if_revision: Option<String>,
    },

    /// Reabrir uma tarefa concluída
    #[command(name = "reabrir", alias = "reopen")]
    Reabrir {
        /// UUID completo ou prefixo de no mínimo 8 caracteres hexadecimais
        id: String,
        /// A referência de 8 caracteres mostrada por `noteit tarefas`
        referencia: String,
        /// Só gravar se a nota ainda estiver nesta revisão
        ///
        /// A revisão vem do campo `revision` de `noteit ler --json`. Sem ela a
        /// gravação é incondicional, como sempre foi.
        #[arg(long = "if-revision", value_name = "REVISAO")]
        if_revision: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum TrashCommand {
    /// Restaurar uma nota da lixeira
    ///
    /// Restaura os dados e nada mais: nenhuma janela é aberta, nenhum foco
    /// muda e nenhuma geometria é alterada.
    #[command(name = "restaurar", alias = "restore")]
    Restaurar {
        /// UUID completo ou prefixo de no mínimo 8 caracteres hexadecimais
        id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_primary_commands_and_aliases() {
        let ajuda = CliArgs::try_parse_from(["noteit", "ajuda"]).expect("ajuda");
        assert_eq!(ajuda.command, Some(CliCommand::Ajuda));

        let help = CliArgs::try_parse_from(["noteit", "help"]).expect("help alias");
        assert_eq!(help.command, Some(CliCommand::Ajuda));

        let versao = CliArgs::try_parse_from(["noteit", "versao"]).expect("versao");
        assert_eq!(versao.command, Some(CliCommand::Versao));

        let version = CliArgs::try_parse_from(["noteit", "version"]).expect("version alias");
        assert_eq!(version.command, Some(CliCommand::Versao));

        let status = CliArgs::try_parse_from(["noteit", "status"]).expect("status");
        assert_eq!(status.command, Some(CliCommand::Status));

        // Read API commands & aliases
        let listar =
            CliArgs::try_parse_from(["noteit", "listar", "--limite", "10", "--tag", "Medicina"])
                .expect("listar");
        assert!(matches!(
            listar.command,
            Some(CliCommand::Listar {
                limite: Some(10),
                ..
            })
        ));

        let list = CliArgs::try_parse_from(["noteit", "list", "--limit", "5"]).expect("list alias");
        assert!(matches!(
            list.command,
            Some(CliCommand::Listar {
                limite: Some(5),
                ..
            })
        ));

        let ler = CliArgs::try_parse_from(["noteit", "ler", "8c4f1a2b"]).expect("ler");
        assert!(matches!(ler.command, Some(CliCommand::Ler { id }) if id == "8c4f1a2b"));

        let read = CliArgs::try_parse_from(["noteit", "read", "8c4f1a2b"]).expect("read alias");
        assert!(matches!(read.command, Some(CliCommand::Ler { id }) if id == "8c4f1a2b"));

        let buscar =
            CliArgs::try_parse_from(["noteit", "buscar", "choque séptico"]).expect("buscar");
        assert!(
            matches!(buscar.command, Some(CliCommand::Buscar { consulta, .. }) if consulta == "choque séptico")
        );

        let search =
            CliArgs::try_parse_from(["noteit", "search", "choque septico"]).expect("search alias");
        assert!(
            matches!(search.command, Some(CliCommand::Buscar { consulta, .. }) if consulta == "choque septico")
        );

        let tags = CliArgs::try_parse_from(["noteit", "tags"]).expect("tags");
        assert_eq!(tags.command, Some(CliCommand::Tags { command: None }));

        let props = CliArgs::try_parse_from(["noteit", "propriedades"]).expect("propriedades");
        assert_eq!(
            props.command,
            Some(CliCommand::Propriedades { command: None })
        );

        let props_en = CliArgs::try_parse_from(["noteit", "properties"]).expect("properties alias");
        assert_eq!(
            props_en.command,
            Some(CliCommand::Propriedades { command: None })
        );

        let tarefas = CliArgs::try_parse_from(["noteit", "tarefas", "--estado", "concluidas"])
            .expect("tarefas");
        assert!(matches!(
            tarefas.command,
            Some(CliCommand::Tarefas {
                estado: TaskStateArg::Concluidas,
                ..
            })
        ));

        let tasks =
            CliArgs::try_parse_from(["noteit", "tasks", "--state", "all"]).expect("tasks alias");
        assert!(matches!(
            tasks.command,
            Some(CliCommand::Tarefas {
                estado: TaskStateArg::Todas,
                ..
            })
        ));

        let lixeira = CliArgs::try_parse_from(["noteit", "lixeira"]).expect("lixeira");
        assert_eq!(lixeira.command, Some(CliCommand::Lixeira { command: None }));

        let trash = CliArgs::try_parse_from(["noteit", "trash"]).expect("trash alias");
        assert_eq!(trash.command, Some(CliCommand::Lixeira { command: None }));
    }

    #[test]
    fn parse_no_command_returns_none() {
        let empty = CliArgs::try_parse_from(["noteit"]).expect("no subcommand");
        assert_eq!(empty.command, None);
    }
}
