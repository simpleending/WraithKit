// Build a deployment folder from explicit, verified destinations. Never deploys or changes source.
// WRAITH_REPOSITORY_URL and WRAITH_SITE_URL may be supplied as CI environment variables.
import { cp, mkdir, readFile, writeFile, stat } from 'node:fs/promises';
import { resolve, dirname, join, relative, isAbsolute, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const usage = 'node site/tools/prepare-release.mjs --repository <public GitHub repository URL> --site-url <final HTTPS site URL> --out <new output folder>';
const args = process.argv.slice(2);
if (args.includes('--help')) { console.log(usage); process.exit(0); }
const values = {};
for (let i = 0; i < args.length; i += 2) {
  if (!['--repository', '--site-url', '--out'].includes(args[i]) || !args[i + 1] || args[i + 1].startsWith('--')) throw new Error(usage);
  values[args[i].slice(2)] = args[i + 1];
}

const repository = values.repository || process.env.WRAITH_REPOSITORY_URL;
const destination = values['site-url'] || process.env.WRAITH_SITE_URL;
if (!repository || !destination || !values.out) throw new Error(`Final repository, site URL and a new output folder are required. ${usage}`);
const repo = new URL(repository);
if (repo.protocol !== 'https:' || repo.hostname !== 'github.com' || repo.username || repo.password || repo.search || repo.hash || !/^\/[\w.-]+\/[\w.-]+\/?$/.test(repo.pathname)) throw new Error('Use the exact HTTPS GitHub repository URL, not a branch or file URL.');
const repoUrl = repo.href.replace(/\/$/, '').replace(/\.git$/, '');
if (repoUrl === 'https://github.com/kidbuu00/wraithTest') throw new Error('The old repository link is not public. Supply the verified final public repository.');
const site = new URL(destination);
if (site.protocol !== 'https:' || site.username || site.password || site.search || site.hash || !/^[\w/-]*$/.test(site.pathname) || /(^|\.)(localhost|example\.com|example\.net|example\.org)$|\.(example|invalid|test)$|^\d+\.\d+\.\d+\.\d+$/.test(site.hostname)) throw new Error('Use the final HTTPS hosting URL, not a local or placeholder address. A project subpath is supported.');
site.pathname = site.pathname.replace(/\/?$/, '/');
const base = site.href;
const source = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const output = resolve(values.out);
const sourceRelativeOutput = relative(source, output);
if (sourceRelativeOutput === '' || (!sourceRelativeOutput.startsWith('..' + sep) && sourceRelativeOutput !== '..' && !isAbsolute(sourceRelativeOutput))) throw new Error('Output must be outside the source site folder.');
try { await stat(output); throw new Error('Output already exists. Choose a new folder; this command never overwrites a build.'); }
catch (error) { if (error.code !== 'ENOENT') throw error; }

await mkdir(output, { recursive: true });
for (const folder of ['assets', 'css', 'fonts', 'js']) await cp(join(source, folder), join(output, folder), { recursive: true });
for (const file of ['index.html', '404.html', 'robots.txt', 'sitemap.xml']) {
  let text = await readFile(join(source, file), 'utf8');
  text = text.replaceAll('https://github.com/kidbuu00/wraithTest', repoUrl).replaceAll('https://github.com/simpleending/WraithKit', repoUrl).replaceAll('https://wraithkit.example/', base);
  if (file === '404.html') {
    text = text.replaceAll('href="/', `href="${site.pathname}`).replaceAll("path: '/assets/", `path: '${site.pathname}assets/`);
  }
  if (text.includes('wraithkit.example') || text.includes('kidbuu00/wraithTest')) throw new Error(`An unresolved destination remains in ${file}.`);
  await writeFile(join(output, file), text);
}
console.log(`Prepared ${output}\nRepository: ${repoUrl}\nSite: ${base}\nNo upload, deployment or public-access check was performed.`);
