// Search passages in the order Russian, Portuguese, French, Spanish, Italian.
pub(crate) fn catalog_texts(id: &str) -> &'static [&'static str] {
    let topic = id
        .strip_suffix(".debian")
        .or_else(|| id.strip_suffix(".arch"))
        .unwrap_or(id);
    match topic {
        "audio.output.volume" => &[
            "Настройка громкости звука и выбор динамиков или наушников. Сделать звук громче, тише или отключить его.",
            "Ajustar o volume de saída e selecionar alto-falantes ou fones de ouvido. Aumentar, baixar ou silenciar o som.",
            "Régler le volume sonore et choisir les haut-parleurs ou le casque. Augmenter, baisser ou couper le son.",
            "Ajustar el volumen de salida y elegir altavoces o auriculares. Subir, bajar o silenciar el sonido.",
            "Regolare il volume e scegliere altoparlanti o cuffie. Alzare, abbassare o disattivare l’audio.",
        ],
        "display.brightness" => &[
            "Настройка яркости экрана. Экран слишком тёмный, как увеличить яркость и сделать дисплей светлее?",
            "Ajustar o brilho da tela. A tela está escura demais, como aumentar o brilho do monitor?",
            "Régler la luminosité de l’écran. Mon écran est trop sombre, comment augmenter la luminosité ?",
            "Ajustar el brillo de la pantalla. Mi pantalla está demasiado oscura, ¿cómo subo el brillo?",
            "Regolare la luminosità dello schermo. Il mio schermo è troppo scuro, come posso aumentare la luminosità?",
        ],
        "network.wifi" => &[
            "Подключение к сети Wi-Fi и управление беспроводным интернетом. Не работает Wi-Fi, найти точку доступа.",
            "Conectar ao Wi-Fi e gerenciar redes sem fio. Resolver falta de internet ou conectar a um ponto de acesso.",
            "Se connecter au Wi-Fi et gérer les réseaux sans fil. Résoudre un problème de connexion ou rejoindre un point d’accès.",
            "Conectarse al wifi y gestionar redes inalámbricas. Resolver problemas de conexión o unirse a un punto de acceso.",
            "Connettersi al Wi-Fi e gestire le reti senza fili. Risolvere problemi di connessione o accedere a un hotspot.",
        ],
        "input.keyboard" => &[
            "Изменить раскладку и язык ввода клавиатуры. Настройки набора текста и переключение языков.",
            "Alterar o layout e o idioma de entrada do teclado. Configurar digitação e alternar idiomas.",
            "Changer la disposition du clavier et la langue de saisie. Régler les préférences de frappe.",
            "Cambiar la distribución del teclado y el idioma de entrada. Ajustar las preferencias de escritura.",
            "Cambiare la disposizione della tastiera e la lingua di input. Configurare le preferenze di digitazione.",
        ],
        "power.battery" => &[
            "Настройки питания и аккумулятора. Включить энергосбережение и продлить работу ноутбука от батареи.",
            "Configurar energia e bateria. Ativar a economia de energia e aumentar a autonomia do notebook.",
            "Régler l’alimentation et la batterie. Activer l’économie d’énergie et prolonger l’autonomie du portable.",
            "Configurar la energía y la batería. Activar el ahorro de energía y prolongar la autonomía del portátil.",
            "Gestire alimentazione e batteria. Attivare il risparmio energetico e prolungare l’autonomia del portatile.",
        ],
        "command.disk.free" => &[
            "Показать свободное и занятое место на подключённых файловых системах и дисках.",
            "Mostrar o espaço livre e ocupado nos discos e sistemas de arquivos montados.",
            "Afficher l’espace disque libre et utilisé sur les systèmes de fichiers montés.",
            "Mostrar el espacio libre y ocupado en discos y sistemas de archivos montados.",
            "Mostrare lo spazio libero e utilizzato sui dischi e sui file system montati.",
        ],
        "command.disk.directory-usage" => &[
            "Определить, какие каталоги занимают больше всего места. Отсортировать папки по размеру.",
            "Descobrir quais pastas ocupam mais espaço. Ordenar diretórios pelo espaço usado.",
            "Trouver les dossiers qui occupent le plus de place. Trier les répertoires par taille.",
            "Averiguar qué carpetas ocupan más espacio. Ordenar los directorios por tamaño.",
            "Scoprire quali cartelle occupano più spazio. Ordinare le directory per dimensione.",
        ],
        "command.files.find-name" => &[
            "Искать файлы по имени или расширению во всех вложенных каталогах.",
            "Buscar arquivos por nome ou extensão em todas as subpastas.",
            "Chercher des fichiers par nom ou extension dans tous les sous-dossiers.",
            "Buscar archivos por nombre o extensión en todas las subcarpetas.",
            "Cercare file per nome o estensione in tutte le sottocartelle.",
        ],
        "command.files.find-large" => &[
            "Найти большие файлы в каталоге и подпапках, например файлы размером более 100 МБ.",
            "Encontrar arquivos grandes nas subpastas, por exemplo, arquivos maiores que 100 MB.",
            "Trouver les gros fichiers dans les sous-dossiers, par exemple ceux de plus de 100 Mo.",
            "Encontrar archivos grandes en subcarpetas, por ejemplo, archivos de más de 100 MB.",
            "Trovare file di grandi dimensioni nelle sottocartelle, per esempio quelli sopra 100 MB.",
        ],
        "command.files.list" => &[
            "Показать файлы текущего каталога, включая скрытые, с размером, датой и правами доступа.",
            "Listar os arquivos da pasta atual, inclusive os ocultos, com tamanho, data e permissões.",
            "Lister les fichiers du dossier courant, y compris les fichiers cachés, avec taille, date et permissions.",
            "Listar los archivos de la carpeta actual, incluidos los ocultos, con tamaño, fecha y permisos.",
            "Elencare i file della cartella corrente, compresi quelli nascosti, con dimensione, data e permessi.",
        ],
        "command.files.current-directory" => &[
            "Показать полный абсолютный путь текущего рабочего каталога.",
            "Mostrar o caminho absoluto do diretório de trabalho atual.",
            "Afficher le chemin absolu du répertoire de travail courant.",
            "Mostrar la ruta absoluta del directorio de trabajo actual.",
            "Mostrare il percorso assoluto della directory di lavoro corrente.",
        ],
        "command.text.find" => &[
            "Найти текст или код ошибки в содержимом файлов и подпапок и вывести номера строк.",
            "Buscar texto ou um código de erro no conteúdo dos arquivos e subpastas e mostrar os números das linhas.",
            "Chercher du texte ou un code d’erreur dans les fichiers et sous-dossiers, avec les numéros de ligne.",
            "Buscar texto o un código de error dentro de archivos y subcarpetas y mostrar los números de línea.",
            "Cercare testo o un codice di errore nel contenuto dei file e delle sottocartelle mostrando i numeri di riga.",
        ],
        "command.text.follow-log" => &[
            "Следить за журналом в реальном времени и видеть новые строки по мере добавления.",
            "Acompanhar um arquivo de log em tempo real conforme novas linhas são adicionadas.",
            "Suivre un fichier journal en direct lorsque de nouvelles lignes y sont ajoutées.",
            "Seguir un archivo de registro en tiempo real a medida que se añaden líneas.",
            "Seguire un file di log in tempo reale quando vengono aggiunte nuove righe.",
        ],
        "command.text.line-count" => &[
            "Подсчитать количество строк в текстовом файле.",
            "Contar o número de linhas em um arquivo de texto.",
            "Compter le nombre de lignes dans un fichier texte.",
            "Contar el número de líneas de un archivo de texto.",
            "Contare il numero di righe di un file di testo.",
        ],
        "command.archive.inspect" => &[
            "Просмотреть список файлов в архиве tar.gz без извлечения.",
            "Listar o conteúdo de um arquivo tar.gz sem extrair os arquivos.",
            "Lister le contenu d’une archive tar.gz sans en extraire les fichiers.",
            "Listar el contenido de un archivo tar.gz sin extraer sus archivos.",
            "Elencare il contenuto di un archivio tar.gz senza estrarre i file.",
        ],
        "command.archive.create" => &[
            "Упаковать каталог в сжатый архив tar.gz.",
            "Compactar uma pasta em um arquivo tar.gz.",
            "Compresser un dossier dans une archive tar.gz.",
            "Comprimir una carpeta en un archivo tar.gz.",
            "Comprimere una cartella in un archivio tar.gz.",
        ],
        "command.system.memory" => &[
            "Показать объём занятой и доступной оперативной памяти и подкачки.",
            "Mostrar a memória RAM usada e disponível e o espaço de swap.",
            "Afficher la mémoire vive utilisée et disponible ainsi que l’espace d’échange.",
            "Mostrar la memoria RAM usada y disponible y el espacio de intercambio.",
            "Mostrare la memoria RAM usata e disponibile e lo spazio di swap.",
        ],
        "command.system.processes" => &[
            "Перечислить запущенные процессы, показать их PID и отсортировать по загрузке процессора.",
            "Listar os processos em execução, mostrar o PID e ordenar pelo uso da CPU.",
            "Lister les processus en cours, afficher leur PID et les trier par consommation CPU.",
            "Listar los procesos en ejecución, mostrar su PID y ordenarlos por uso de CPU.",
            "Elencare i processi in esecuzione, mostrare il PID e ordinare per uso della CPU.",
        ],
        "command.system.disks" => &[
            "Показать диски, SSD, USB-накопители, разделы и точки монтирования.",
            "Listar discos, SSDs, unidades USB, partições e pontos de montagem.",
            "Lister les disques, les SSD, les clés USB, les partitions et les points de montage.",
            "Listar discos, SSD, unidades USB, particiones y puntos de montaje.",
            "Elencare dischi, SSD, unità USB, partizioni e punti di montaggio.",
        ],
        "command.system.service-status" => &[
            "Проверить состояние службы systemd и узнать, запущена ли она.",
            "Verificar o estado de um serviço do systemd e saber se está em execução.",
            "Vérifier l’état d’un service systemd et savoir s’il est en cours d’exécution.",
            "Consultar el estado de un servicio de systemd y comprobar si está en ejecución.",
            "Controllare lo stato di un servizio systemd e verificare se è in esecuzione.",
        ],
        "command.system.boot-logs" => &[
            "Просмотреть системный журнал systemd за текущую загрузку.",
            "Consultar o log do systemd da inicialização atual.",
            "Consulter le journal systemd du démarrage actuel.",
            "Consultar el registro de systemd del arranque actual.",
            "Consultare il registro di systemd dell’avvio corrente.",
        ],
        "command.network.addresses" => &[
            "Показать локальные IP-адреса и сетевые интерфейсы компьютера.",
            "Mostrar os endereços IP locais e as interfaces de rede.",
            "Afficher les adresses IP locales et les interfaces réseau.",
            "Mostrar las direcciones IP locales y las interfaces de red.",
            "Mostrare gli indirizzi IP locali e le interfacce di rete.",
        ],
        "command.network.ports" => &[
            "Показать открытые локальные порты TCP и UDP, которые слушают службы.",
            "Listar as portas TCP e UDP locais em que serviços estão escutando.",
            "Lister les ports TCP et UDP locaux sur lesquels des services sont à l’écoute.",
            "Listar los puertos TCP y UDP locales en los que escuchan servicios.",
            "Elencare le porte TCP e UDP locali su cui sono in ascolto i servizi.",
        ],
        "command.help.manual" => &[
            "Открыть справочную страницу Linux и узнать синтаксис и параметры команды.",
            "Abrir a página de manual de um comando Linux e consultar a sintaxe e as opções.",
            "Ouvrir la page de manuel d’une commande Linux pour consulter la syntaxe et les options.",
            "Abrir la página de manual de un comando de Linux para consultar la sintaxis y las opciones.",
            "Aprire la pagina di manuale di un comando Linux per consultarne sintassi e opzioni.",
        ],
        "command.packages.search" => &[
            "Поиск доступных пакетов в репозиториях по имени или ключевому слову.",
            "Pesquisar pacotes disponíveis nos repositórios por nome ou palavra-chave.",
            "Rechercher des paquets disponibles dans les dépôts par nom ou par mot-clé.",
            "Buscar paquetes disponibles en los repositorios por nombre o palabra clave.",
            "Cercare pacchetti disponibili nei repository per nome o parola chiave.",
        ],
        "command.packages.install" => &[
            "Установить новый пакет и его зависимости из репозиториев. Как установить программу?",
            "Instalar um novo pacote e suas dependências a partir dos repositórios. Como instalo um programa?",
            "Installer un nouveau paquet et ses dépendances depuis les dépôts. Comment installer un logiciel ?",
            "Instalar un paquete nuevo y sus dependencias desde los repositorios. ¿Cómo instalo un programa?",
            "Installare un nuovo pacchetto e le sue dipendenze dai repository. Come installo un programma?",
        ],
        "command.packages.upgrade" => &[
            "Обновить операционную систему и установленные пакеты до доступных версий.",
            "Atualizar o sistema e instalar as atualizações disponíveis para os pacotes instalados.",
            "Mettre à jour le système et appliquer les mises à jour disponibles aux paquets installés.",
            "Actualizar el sistema y aplicar las actualizaciones disponibles para los paquetes instalados.",
            "Aggiornare il sistema e applicare gli aggiornamenti disponibili ai pacchetti installati.",
        ],
        "command.packages.list" => &[
            "Вывести список всех установленных в системе пакетов.",
            "Listar todos os pacotes já instalados no sistema.",
            "Lister tous les paquets déjà installés sur le système.",
            "Listar todos los paquetes instalados en el sistema.",
            "Elencare tutti i pacchetti già installati nel sistema.",
        ],
        "command.packages.details" => &[
            "Показать сведения об известном пакете, включая версию, описание и зависимости.",
            "Consultar a versão, a descrição e as dependências de um pacote conhecido.",
            "Afficher la version, la description et les dépendances d’un paquet connu.",
            "Consultar la versión, la descripción y las dependencias de un paquete conocido.",
            "Mostrare la versione, la descrizione e le dipendenze di un pacchetto noto.",
        ],
        "command.packages.owner" => &[
            "Узнать, какому установленному пакету принадлежит файл или программа.",
            "Descobrir a qual pacote instalado pertence um arquivo ou programa.",
            "Trouver le paquet installé auquel appartient un fichier ou un programme.",
            "Averiguar a qué paquete instalado pertenece un archivo o programa.",
            "Scoprire a quale pacchetto installato appartiene un file o un programma.",
        ],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::catalog_texts;
    use crate::{catalog::SETTINGS, commands::COMMANDS};

    #[test]
    fn every_builtin_entry_has_five_translations() {
        for id in SETTINGS
            .iter()
            .map(|setting| setting.id)
            .chain(COMMANDS.iter().map(|command| command.id))
        {
            let passages = catalog_texts(id);
            assert_eq!(passages.len(), 5, "{id}");
            assert!(passages.iter().all(|text| !text.trim().is_empty()), "{id}");
        }
    }
}
