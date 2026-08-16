//! Subir junto com a sessão do Windows.
//!
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, e só isso.
//!
//! ## Por que esta chave, e não as outras quatro maneiras
//!
//! O Windows tem muitos jeitos de iniciar um programa no login, e quase todos
//! são piores aqui:
//!
//! * **Serviço do Windows** — está proibido, e com razão. Serviços rodam na
//!   sessão 0, sem acesso à área de trabalho do usuário. O Ditador precisa do
//!   microfone, da área de transferência, do teclado e de desenhar na tela: as
//!   quatro coisas que a sessão 0 não tem. Seria construir uma ponte para
//!   atravessar um rio que não existe.
//! * **Tarefa agendada** — funciona, mas pede privilégio para criar e some da
//!   lista de "Aplicativos de Inicialização" do Gerenciador de Tarefas. O
//!   usuário perde o interruptor que ele conhece.
//! * **Pasta Inicializar** (`shell:startup`) — é um atalho `.lnk`, que exige
//!   montar um objeto COM (`IShellLink`) para escrever. Mais código para o mesmo
//!   efeito, e um arquivo que o usuário pode mover sem querer.
//! * **`StartupTask` do MSIX** — é o caminho certo **quando o aplicativo estiver
//!   empacotado**, e está previsto para o marco de empacotamento. Ele não serve
//!   ao desenvolvimento e à instalação avulsa, que é o que existe agora.
//!
//! A chave `Run` do usuário atende a tudo o que se quer: não precisa de
//! administrador, vale só para esta conta, aparece em **Configurações → Aplicativos
//! → Inicializar** e no Gerenciador de Tarefas (onde o usuário pode desligá-la
//! sem passar pelo Ditador), e é uma linha de texto que se remove com um clique.
//!
//! ## O caminho do executável vai entre aspas
//!
//! Pelo mesmo motivo que o `.desktop` do Linux cita o `Exec=`: um caminho com
//! espaço — que é o caso de qualquer instalação em `C:\Program Files\…` ou na
//! pasta do usuário — seria partido no primeiro espaço, e o Windows tentaria
//! executar `C:\Program`. É o bug clássico de instalador, e ele nasce aqui.

use anyhow::{Context, Result, bail};
use std::ffi::OsStr;
use std::os::windows::ffi::{OsStrExt, OsStringExt};

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};

/// A chave que o Windows lê no login de cada usuário.
const CHAVE: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// O nome do valor. É ele que aparece na lista de inicialização do Gerenciador
/// de Tarefas, então é um nome para pessoa ler, não um identificador.
const VALOR: &str = "Ditador";

/// Está armado para subir com a sessão?
pub fn ligado() -> bool {
    ler().is_some()
}

/// Arma ou desarma.
pub fn definir(ligar: bool) -> Result<()> {
    if ligar { escrever() } else { apagar() }
}

/// A explicação que a tela de configurações mostra embaixo do interruptor.
pub fn explicacao() -> &'static str {
    "Pela chave de inicialização da sua conta do Windows. Vale na hora, sem precisar \
     salvar. Você também pode desligar em Configurações → Aplicativos → Inicializar."
}

// ------------------------------------------------------------------- o registro

fn abrir(acesso: u32) -> Result<Chave> {
    let caminho = utf16(CHAVE);
    let mut chave: HKEY = std::ptr::null_mut();
    let r = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, caminho.as_ptr(), 0, acesso, &mut chave) };
    if r != ERROR_SUCCESS {
        bail!(
            "abrindo HKCU\\{CHAVE}: {}",
            std::io::Error::from_raw_os_error(r as i32)
        );
    }
    Ok(Chave(chave))
}

/// Abre a chave `Run` para escrita, criando-a se ela não existir.
///
/// Ela existe em qualquer Windows com algum tempo de uso, mas **não é garantida**:
/// num perfil recém-criado, em que nada nunca pediu para iniciar com a sessão,
/// ela simplesmente não está lá. Foi assim que a CI a encontrou, num agente
/// limpo do GitHub, e o `RegOpenKeyExW` respondeu o que responde nesse caso —
/// "o sistema não pode encontrar o arquivo especificado", que é uma frase
/// difícil de ligar a um interruptor de inicialização.
///
/// `RegCreateKeyExW` abre a existente ou cria a que falta, que é exatamente a
/// semântica desejada aqui: quem liga o interruptor quer a chave lá, tendo ela
/// existido antes ou não. Criar uma subchave do próprio usuário em `HKCU` não
/// pede privilégio nenhum.
fn abrir_ou_criar() -> Result<Chave> {
    let caminho = utf16(CHAVE);
    let mut chave: HKEY = std::ptr::null_mut();
    let r = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            caminho.as_ptr(),
            0,
            std::ptr::null(),
            // Não volátil: tem de sobreviver ao reinício, que é o ponto.
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            std::ptr::null(),
            &mut chave,
            // Não interessa saber se foi criada agora ou já existia.
            std::ptr::null_mut(),
        )
    };
    if r != ERROR_SUCCESS {
        bail!(
            "abrindo (ou criando) HKCU\\{CHAVE}: {}",
            std::io::Error::from_raw_os_error(r as i32)
        );
    }
    Ok(Chave(chave))
}

/// Fecha a chave ao sair de escopo. Sem isto, cada leitura da tela de
/// configurações — que o egui repinta várias vezes por segundo — vazaria um
/// handle de registro.
struct Chave(HKEY);

impl Drop for Chave {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

fn ler() -> Option<String> {
    let chave = abrir(KEY_READ).ok()?;

    let nome = utf16(VALOR);
    let mut tipo = 0u32;
    let mut bytes = 0u32;
    let r = unsafe {
        RegQueryValueExW(
            chave.0,
            nome.as_ptr(),
            std::ptr::null(),
            &mut tipo,
            std::ptr::null_mut(),
            &mut bytes,
        )
    };
    if r != ERROR_SUCCESS || bytes == 0 {
        return None;
    }

    let mut buffer = vec![0u16; bytes as usize / 2 + 1];
    let mut bytes_lidos = bytes;
    let r = unsafe {
        RegQueryValueExW(
            chave.0,
            nome.as_ptr(),
            std::ptr::null(),
            &mut tipo,
            buffer.as_mut_ptr().cast(),
            &mut bytes_lidos,
        )
    };
    if r != ERROR_SUCCESS {
        return None;
    }

    // O registro não promete terminador nulo; cortar no primeiro (ou no fim)
    // evita arrastar lixo do buffer para dentro da string.
    let fim = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(
        std::ffi::OsString::from_wide(&buffer[..fim])
            .to_string_lossy()
            .into_owned(),
    )
}

fn escrever() -> Result<()> {
    let chave = abrir_ou_criar()?;
    let comando = citar(&quem_deve_subir()?);
    let dados = utf16(&comando);
    let nome = utf16(VALOR);

    let r = unsafe {
        RegSetValueExW(
            chave.0,
            nome.as_ptr(),
            0,
            REG_SZ,
            dados.as_ptr().cast(),
            // Em bytes, e **incluindo** o terminador nulo. Sem ele, o Windows
            // lê a string até onde o buffer acabar e o comando sai com lixo no
            // fim — que é um dos jeitos de o autostart falhar em silêncio.
            (dados.len() * 2) as u32,
        )
    };
    if r != ERROR_SUCCESS {
        bail!(
            "gravando HKCU\\{CHAVE}\\{VALOR}: {}",
            std::io::Error::from_raw_os_error(r as i32)
        );
    }
    Ok(())
}

fn apagar() -> Result<()> {
    let chave = match abrir(KEY_WRITE) {
        Ok(chave) => chave,
        // A chave `Run` pode não existir — num perfil novo em que nada nunca
        // pediu para iniciar com a sessão, ela não está lá. E aí também não há
        // nada armado, que é o resultado que se queria. Desarmar é a única das
        // duas operações que pode se dar por satisfeita sem tocar no registro.
        Err(_) => return Ok(()),
    };
    let nome = utf16(VALOR);
    let r = unsafe { RegDeleteValueW(chave.0, nome.as_ptr()) };
    if r != ERROR_SUCCESS && r != ERROR_FILE_NOT_FOUND {
        bail!(
            "removendo HKCU\\{CHAVE}\\{VALOR}: {}",
            std::io::Error::from_raw_os_error(r as i32)
        );
    }
    Ok(())
}

/// O caminho absoluto do binário em execução.
///
/// O Windows pode iniciar o programa com um diretório de trabalho qualquer, e um
/// comando relativo no registro simplesmente não sobe.
fn executavel_atual() -> Result<std::path::PathBuf> {
    std::env::current_exe().context("descobrindo o caminho do próprio binário")
}

/// O nome do frontend, ao lado do backend numa instalação normal.
const FRONTEND: &str = "Ditador.Windows.exe";

/// Quem deve subir no login: o frontend, se ele estiver instalado; o backend, se
/// não estiver.
///
/// A chave `Run` tem **um** valor chamado `Ditador`, e dois programas o escrevem:
/// o `instalar.ps1` (com `-IniciarComOWindows`) e este módulo, quando alguém liga
/// o interruptor na tela de configurações. O último a escrever vence — então, se
/// os dois não apontarem para o mesmo lugar, ligar o interruptor por dentro
/// trocaria silenciosamente o que sobe no login.
///
/// E a troca não seria inofensiva. Subindo só o `ditador.exe`, o ditado funciona
/// — atalho, áudio, Whisper, área de transferência —, mas não há ícone na área de
/// notificação nem aviso na tela, porque quem desenha as duas coisas no Windows é
/// o frontend. O usuário ligaria "iniciar com o Windows" e perderia a interface.
///
/// O contrário é seguro: o frontend sobe o backend sozinho quando percebe que ele
/// não está no ar, e faz isso **uma vez**. Por isso, havendo os dois, quem entra
/// na chave é o frontend — a mesma escolha que o `instalar.ps1` faz, e pelo mesmo
/// motivo.
fn quem_deve_subir() -> Result<String> {
    Ok(escolher(&executavel_atual()?).display().to_string())
}

/// A decisão em si, separada para poder ser testada sem depender de onde o
/// binário do teste está.
fn escolher(eu: &std::path::Path) -> std::path::PathBuf {
    // `ditador.exe` e `Ditador.Windows.exe` moram na mesma pasta desde o
    // `instalar.ps1`. Não havendo o frontend ali — quem compilou só o Rust, ou
    // roda o binário de dentro do `target\release` —, sobe o backend mesmo, que
    // é melhor do que não subir nada.
    let frontend = eu.with_file_name(FRONTEND);
    if frontend.is_file() {
        frontend
    } else {
        eu.to_path_buf()
    }
}

/// Põe o comando entre aspas, para que um caminho com espaço não seja partido.
///
/// Não há escape a fazer dentro: o Windows não permite aspas em nome de arquivo,
/// então o conteúdo nunca pode fechar as aspas por conta própria. É a diferença
/// para o `.desktop` do Linux, onde `$`, crase e barra invertida precisam de
/// tratamento.
fn citar(comando: &str) -> String {
    format!("\"{comando}\"")
}

fn utf16(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_caminho_com_espaco_sai_entre_aspas() {
        // O caso normal de qualquer instalação no Windows.
        assert_eq!(
            citar(r"C:\Program Files\Ditador\ditador.exe"),
            "\"C:\\Program Files\\Ditador\\ditador.exe\""
        );
    }

    #[test]
    fn havendo_frontend_ao_lado_e_ele_quem_sobe_no_login() {
        // O interruptor da tela de configurações e o `instalar.ps1` escrevem no
        // mesmo valor da chave `Run`. Apontando para lugares diferentes, o
        // último a escrever venceria — e ligar o interruptor por dentro deixaria
        // a sessão subindo o backend sem o ícone e sem o aviso na tela.
        let pasta = std::env::temp_dir().join("ditador-teste-autostart");
        let _ = std::fs::create_dir_all(&pasta);
        let backend = pasta.join("ditador.exe");
        let frontend = pasta.join(FRONTEND);

        let _ = std::fs::remove_file(&frontend);
        assert_eq!(
            escolher(&backend),
            backend,
            "sem o frontend instalado, quem sobe é o próprio backend"
        );

        std::fs::write(&frontend, b"").expect("criando um frontend de mentira");
        assert_eq!(
            escolher(&backend),
            frontend,
            "havendo frontend, é ele que sobe — ele acorda o backend sozinho"
        );

        let _ = std::fs::remove_file(&frontend);
        let _ = std::fs::remove_dir(&pasta);
    }

    #[test]
    fn armar_e_desarmar_deixa_o_registro_como_estava() {
        // Mexe no registro de verdade, na chave de verdade — é o único jeito de
        // provar que o valor gravado é o que o Windows vai ler no login. Por
        // isso ele restaura o estado anterior no fim, inclusive se o usuário já
        // tiver o Ditador armado.
        let antes = ler();

        definir(true).expect("armando o início automático");
        let gravado = ler().expect("o valor não apareceu no registro");
        assert!(
            gravado.starts_with('"') && gravado.ends_with('"'),
            "o comando não saiu entre aspas: {gravado}"
        );
        assert!(ligado());

        definir(false).expect("desarmando");
        assert!(
            !ligado(),
            "o valor continuou no registro depois de desarmar"
        );

        // Desarmar duas vezes não é erro: é o estado desejado, já alcançado.
        definir(false).expect("desarmar de novo precisa ser inofensivo");

        if let Some(valor) = antes {
            let chave = abrir(KEY_WRITE).expect("reabrindo para restaurar");
            let dados = utf16(&valor);
            let nome = utf16(VALOR);
            unsafe {
                RegSetValueExW(
                    chave.0,
                    nome.as_ptr(),
                    0,
                    REG_SZ,
                    dados.as_ptr().cast(),
                    (dados.len() * 2) as u32,
                )
            };
        }
    }
}
