// Récupère xdelta3 depuis sa release officielle et le dépose dans bin/.
//
// Les patchs des jeux rétro sont des .xdelta compressés en DJW (le Huffman
// statique interne de xdelta3) : aucune bibliothèque JavaScript ne sait les
// décoder, il faut le vrai binaire. On le télécharge au build plutôt que de le
// commiter, pour garder le dépôt propre.
//
// xdelta3 est sous licence GPL-2.0 : il est lancé comme un programme séparé,
// et son code source doit être proposé aux utilisateurs (voir bin/LISEZ-MOI.txt).

const fs = require('fs');
const path = require('path');
const https = require('https');
const { execFileSync } = require('child_process');

const XDELTA_VERSION = '3.1.0';
const ARCHIVE_NAME = `xdelta3-${XDELTA_VERSION}-x86_64.exe.zip`;
const ARCHIVE_URL = `https://github.com/jmacd/xdelta-gpl/releases/download/v${XDELTA_VERSION}/${ARCHIVE_NAME}`;

const binFolder = path.join(__dirname, '..', 'bin');
const targetPath = path.join(binFolder, 'xdelta3.exe');
const archivePath = path.join(binFolder, ARCHIVE_NAME);

function download(url, destination) {
    return new Promise((resolve, reject) => {
        https.get(url, response => {
            // Les releases GitHub redirigent vers un CDN
            if (response.statusCode === 301 || response.statusCode === 302) {
                response.resume();
                return resolve(download(response.headers.location, destination));
            }

            if (response.statusCode !== 200) {
                response.resume();
                return reject(new Error(`${url} a répondu ${response.statusCode}`));
            }

            const file = fs.createWriteStream(destination);
            response.pipe(file);
            file.on('finish', () => file.close(resolve));
            file.on('error', reject);
        }).on('error', reject);
    });
}

async function main() {
    if (fs.existsSync(targetPath)) {
        console.log('xdelta3 est déjà présent dans bin/, rien à faire.');
        return;
    }

    fs.mkdirSync(binFolder, { recursive: true });

    console.log(`Téléchargement de ${ARCHIVE_NAME}...`);
    await download(ARCHIVE_URL, archivePath);

    console.log('Extraction...');
    // Expand-Archive est disponible partout sur Windows, ça évite une dépendance
    execFileSync('powershell', [
        '-NoProfile', '-Command',
        `Expand-Archive -LiteralPath '${archivePath}' -DestinationPath '${binFolder}' -Force`
    ]);

    // L'archive contient xdelta3-<version>-x86_64.exe, on normalise le nom
    const extracted = fs.readdirSync(binFolder)
        .find(name => name.startsWith('xdelta3-') && name.endsWith('.exe'));

    if (!extracted)
        throw new Error("Aucun exécutable xdelta3 trouvé dans l'archive.");

    fs.renameSync(path.join(binFolder, extracted), targetPath);
    fs.unlinkSync(archivePath);

    console.log(`xdelta3 ${XDELTA_VERSION} installé dans bin/xdelta3.exe`);
}

main().catch(error => {
    console.error('Échec de la récupération de xdelta3 :', error.message);
    process.exit(1);
});
