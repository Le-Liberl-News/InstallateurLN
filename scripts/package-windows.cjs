// Build an electron-updater-compatible NSIS installer around the native binary.
// Never publishes; release assets are uploaded only by the explicit tag workflow.
const fs=require('node:fs');
const path=require('node:path');
const {spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..');
const version=fs.readFileSync(path.join(root,'Cargo.toml'),'utf8').match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if(!/^\d+\.\d+\.\d+$/.test(version||''))throw Error('Invalid Cargo package version');
if(process.env.GITHUB_REF_TYPE==='tag' && process.env.GITHUB_REF_NAME!==`v${version}`)throw Error('Release tag must match Cargo.toml version');
const value=(name,fallback)=>{const i=process.argv.indexOf(name);return i<0?fallback:process.argv[i+1];};
const source=path.resolve(value('--exe',path.join(root,'target/release/InstallateurLiberlNews.exe')));
const output=path.resolve(value('--out',path.join(root,'target/windows-setup')));
const project=path.join(root,'packaging/windows');
const staging=path.join(root,'target/windows-native-payload');
if(!fs.statSync(source).isFile())throw Error('Native executable missing');
fs.mkdirSync(staging,{recursive:true});
fs.mkdirSync(path.join(staging,'resources'),{recursive:true});
// Same executable filename as the old Electron application: existing shortcuts survive.
fs.copyFileSync(source,path.join(staging,'Installateur du Liberl News.exe'));
const config={
  appId:'com.liberlnews.installer',
  productName:'Installateur du Liberl News',
  executableName:'Installateur du Liberl News',
  electronVersion:'38.8.6', // Framework metadata only; --prepackaged bundles no Electron files.
  extraMetadata:{version},
  directories:{output},
  publish:[{provider:'github',owner:'Le-Liberl-News',repo:'InstallateurLN'}],
  win:{icon:path.join(root,'assets/icon.ico'),target:[{target:'nsis',arch:['x64']}],signAndEditExecutable:false},
  nsis:{oneClick:true,perMachine:false,runAfterFinish:true,createDesktopShortcut:true,createStartMenuShortcut:true,
    shortcutName:'Installateur du Liberl News',artifactName:'Installateur-du-Liberl-News-Setup-${version}.${ext}',deleteAppDataOnUninstall:false},
};
const configPath=path.join(root,'target/windows-installer-config.json');
fs.writeFileSync(configPath,JSON.stringify(config,null,2));
const builder=value('--builder',null)||require.resolve('electron-builder/cli.js',{paths:[project]});
const result=spawnSync(process.execPath,[builder,'--projectDir',project,'--prepackaged',staging,'--config',configPath,'--win','nsis','--x64','--publish','never'],{stdio:'inherit',env:{...process.env,CSC_IDENTITY_AUTO_DISCOVERY:'false'}});
if(result.error)throw result.error;
process.exitCode=result.status??1;
