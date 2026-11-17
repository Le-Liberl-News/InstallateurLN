// Exercise the same NsisUpdater as version 1.2.1, with an isolated local feed.
// Stops at the installer handoff: never installs, quits or edits the user's app.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const os=require('node:os');
const http=require('node:http');
const crypto=require('node:crypto');
const {createRequire}=require('node:module');
const {spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..');
const arg=(name,fallback)=>{const i=process.argv.indexOf(name);return i<0?fallback:process.argv[i+1];};
const modules=arg('--modules',null);
const load=createRequire(modules?path.join(path.resolve(modules),'../package.json'):path.join(root,'packaging/windows/package.json'));
const yaml=load('js-yaml');
const directory=path.resolve(arg('--setup-dir',path.join(root,'target/windows-setup')));
const source=path.resolve(arg('--exe',path.join(root,'target/release/InstallateurLiberlNews.exe')));
const manifest=yaml.load(fs.readFileSync(path.join(directory,'latest.yml'),'utf8'));
const setup=path.join(directory,manifest.path);
const bytes=fs.readFileSync(setup);
assert.equal(crypto.createHash('sha512').update(bytes).digest('base64'),manifest.sha512);
assert.equal(manifest.files[0].size,bytes.length);
const config=JSON.parse(fs.readFileSync(path.join(root,'target/windows-installer-config.json'),'utf8'));
assert.equal(config.appId,'com.liberlnews.installer');
assert.equal(config.productName,'Installateur du Liberl News');
assert.equal(config.executableName,'Installateur du Liberl News');
assert.equal(config.extraMetadata.version,manifest.version);
assert.equal(config.nsis.perMachine,false);
assert.equal(config.nsis.deleteAppDataOnUninstall,false);
const {UUID}=load('builder-util-runtime');
assert.equal(UUID.v5(config.appId,UUID.parse('50e065bc-3134-11e6-9bab-38c9862bdaf3')),'4b43ab33-61ca-59c6-b165-8140fc87bcc9');
const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'ln-legacy-bridge-'));
let server;
(async()=>{
  const unpacked=path.join(temporary,'payload');
  const extraction=spawnSync(load('7zip-bin').path7za,['x','-y',`-o${unpacked}`,setup],{encoding:'utf8'});
  assert([0,1].includes(extraction.status),extraction.stderr);
  assert.deepEqual(fs.readFileSync(path.join(unpacked,'Installateur du Liberl News.exe')),fs.readFileSync(source));
  assert(!fs.existsSync(path.join(unpacked,'resources/app.asar')));
  server=http.createServer((req,res)=>{
    const name=path.basename(new URL(req.url,'http://localhost').pathname);
    if(!['latest.yml',manifest.path].includes(name)){res.writeHead(404);res.end();return;}
    const content=fs.readFileSync(path.join(directory,name));
    res.writeHead(200,{'Content-Length':content.length});res.end(content);
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const feed=`http://127.0.0.1:${server.address().port}`;
  const updateConfig=path.join(temporary,'app-update.yml');
  fs.writeFileSync(updateConfig,`provider: generic\nurl: ${feed}\nupdaterCacheDirName: fixture-updater\n`);
  const {NsisUpdater}=load('electron-updater/out/NsisUpdater');
  const {NodeHttpExecutor}=load('builder-util/out/nodeHttpExecutor');
  const {ElectronHttpExecutor}=load('electron-updater/out/electronHttpExecutor');
  let onQuit;
  const app={version:'1.2.1',name:'setupliberlnews',isPackaged:true,userDataPath:temporary,baseCachePath:temporary,appUpdateConfigPath:updateConfig,
    whenReady:async()=>{},onQuit:handler=>{onQuit=handler;},quit:()=>assert.fail('Must not quit a real app'),relaunch:()=>assert.fail('Must not relaunch')};
  const updater=new NsisUpdater(undefined,app);
  updater.httpExecutor=new NodeHttpExecutor();
  updater.httpExecutor.download=ElectronHttpExecutor.prototype.download;
  updater.setFeedURL({provider:'generic',url:feed});
  updater.autoDownload=false;
  updater.autoInstallOnAppQuit=true;
  updater.disableDifferentialDownload=true;
  updater.disableWebInstaller=true;
  const result=await updater.checkForUpdates();
  assert.equal(result.updateInfo.version,manifest.version);
  const downloaded=await updater.downloadUpdate();
  assert.deepEqual(fs.readFileSync(downloaded[0]),bytes);
  assert.equal(typeof onQuit,'function');
  let handoff;
  updater.doInstall=options=>{handoff=options;return true;};
  onQuit(0);
  assert.equal(handoff.isSilent,true);
  console.log('Legacy 1.2.1 updater: detects the bridge, verifies/downloads it and hands off on quit. Native payload and installer identity verified. Setup was NOT executed.');
})().catch(error=>{console.error(error);process.exitCode=1;}).finally(async()=>{
  if(server)await new Promise(resolve=>server.close(resolve));
  const resolved=path.resolve(temporary);
  if(!resolved.startsWith(path.resolve(os.tmpdir())+path.sep))throw Error('Invalid temporary cleanup path');
  fs.rmSync(resolved,{recursive:true,force:true});
});
