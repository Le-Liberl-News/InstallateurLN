// Fichier principal lancé au lancement de l'application
const { app, BrowserWindow, ipcMain } = require("electron");
const electron = require('electron');
const path = require('path');
const os = require("os");
const config = require("./package.json");
// Défini la taille de la fenêtre ; une plus grande valeur signifie une fenêtre plus grande (Défaut : 1.1)
const multiplicator = 1.1;

const remoteMain = require('@electron/remote/main');

remoteMain.initialize();

// Mise à jour automatique (build NSIS uniquement : la version portable ne peut
// pas se remplacer elle-même). electron-updater va chercher la dernière release
// GitHub et ne télécharge que le différentiel, l'utilisateur n'a plus à
// re-télécharger l'installateur complet à chaque version.
const checkForUpdates = () => {
    try {
        const { autoUpdater } = require('electron-updater');

        autoUpdater.autoDownload = true;
        autoUpdater.on('error', err => console.log("Mise à jour impossible :", err.message));
        autoUpdater.on('update-downloaded', () => {
            // On installe au prochain démarrage pour ne pas couper un téléchargement de patch en cours
            autoUpdater.autoInstallOnAppQuit = true;
        });
        autoUpdater.checkForUpdates();
    }
    catch (err) {
        console.log("Vérification des mises à jour ignorée :", err.message);
    }
};

const loadMainWindow = async () => {

    // Permet d'obtenir un effet 16:9 adapté à la taille de la résolution de l'utilisateur
    let widthScreen = Math.floor((electron.screen.getPrimaryDisplay().bounds['height'] * multiplicator) / 0.85);
    let heightScreen = Math.floor(widthScreen * 0.56);
	
    mainWindow = new BrowserWindow({
        width : widthScreen,
        height: heightScreen,
        icon: path.join(__dirname, "/images/icon.png"),
        resizable: false, // False = La fenêtre ne peut pas redimensionnée par l'utilisateur
        webPreferences: {
            nodeIntegration: true, // Nécessaire pour activer jQuery et autres modules de Node
            allowRunningInsecureContent: true,
            defaultEncoding: 'UTF-8',
            contextIsolation: false,
            enableRemoteModule: true
        }
    });

    // Les erreurs de la page arrivent dans la console du terminal : indispensable
    // pour diagnostiquer un écran blanc sans avoir à ouvrir les outils de dev
    mainWindow.webContents.on('console-message', (...args) => {
        const details = args[0];
        if (details && typeof details === 'object' && 'message' in details)
            console.log('[page] ' + details.message + ' (' + details.sourceId + ':' + details.lineNumber + ')');
        else
            console.log('[page] ' + args[2] + ' (' + args[4] + ':' + args[3] + ')');
    });

    mainWindow.removeMenu(); // Pas de "Menu", "Options" etc...
	remoteMain.enable(mainWindow.webContents);
    mainWindow.setTitle("Installateur - Liberl News - Version " + config["version"]);
    //mainWindow.webContents.openDevTools({ mode: 'detach' }); // Décommenter cette ligne pour afficher les options de développeur ; utiles pour régler l'interface
	mainWindow.loadFile(path.join(__dirname, "loading.html")); // Chargement de la page html principale
}

app.on("ready", () => {
    loadMainWindow();
    checkForUpdates();
});

ipcMain.handle('exit', async () => {
    
    if (process.platform !== "darwin") {
        app.quit();
    }

});

app.on("window-all-closed", () => {
    if (process.platform !== "darwin") {
      app.quit();
    }
 });

 app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) {
        loadMainWindow();
    }
});