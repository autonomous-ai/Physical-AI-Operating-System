from pathlib import Path
import hashlib,json,subprocess,sys
base=Path('/private/tmp/tests-823-native-20261016')
kind=sys.argv[1]
run='38820115320';repo='autonomous-ai/lampOS';head='8af53fd459795029f6277679ded49bf7826def73'
commands={
 'log':(['gh','run','view',run,'-R',repo,'--log'],'.log'),
 'run-final':(['gh','run','view',run,'-R',repo,'--json','databaseId,headSha,event,status,conclusion,createdAt,startedAt,updatedAt,attempt,url,name,workflowName,jobs'],'.run-final.json'),
 'run-api':(['gh','api',f'repos/{repo}/actions/runs/{run}'],'.run-api.json'),
 'jobs-api':(['gh','api',f'repos/{repo}/actions/runs/{run}/jobs?filter=all&per_page=100'],'.jobs-api.json'),
 'commit-api':(['gh','api',f'repos/{repo}/git/commits/{head}'],'.commit-api.json'),
}
command,suffix=commands[kind];tag='.retrieved' if len(sys.argv)>2 and sys.argv[2]=='retrieved' else '';p=base/('909-'+run+tag+suffix);err=p.with_name(p.name+'.stderr');record=base/('909-'+run+tag+'.capture-'+kind+'.json')
for x in [p,err,record]:assert not x.exists(),x
date=lambda:subprocess.check_output(['date','+%Y-%m-%d %H:%M:%S %z'],text=True).strip()
start=date()
with p.open('xb') as out,err.open('xb') as error: r=subprocess.run(command,stdout=out,stderr=error)
end=date();meta={'command':command,'started_from_date':start,'finished_from_date':end,'exit_code':r.returncode,'stdout':str(p),'stdout_bytes':p.stat().st_size,'stdout_sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'stderr':str(err),'stderr_bytes':err.stat().st_size,'stderr_sha256':hashlib.sha256(err.read_bytes()).hexdigest()};record.write_text(json.dumps(meta,indent=2)+'\n');print(json.dumps(meta))
raise SystemExit(r.returncode)
