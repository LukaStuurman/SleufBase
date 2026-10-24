"""Execute only original pure job parsers, never a client/session constructor."""
import ast,hashlib,json,re
from dataclasses import dataclass
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
source=(ROOT/'kickthemap.py').read_text(encoding='utf-8')
tree=ast.parse(source)
names={'_parse_job_projects','_fallback_project_identity','_parse_prefix','_format_address','_format_user_id','_format_municipality','_format_coordinates','_parse_bool','_extract_js_value','_find_js_array_end'}
job=next(n for n in tree.body if isinstance(n,ast.ClassDef) and n.name=='KickTheMapJob')
cls=next(n for n in tree.body if isinstance(n,ast.ClassDef) and n.name=='KickTheMapClient')
cls.body=[n for n in cls.body if isinstance(n,ast.FunctionDef) and n.name in names]
env=dict(dataclass=dataclass,re=re,json=json,PREFIX_PATTERN=re.compile(r'^(?P<email>.+)_(?P<date>\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2})$'),KickTheMapError=RuntimeError)
exec(compile(ast.Module(body=[job,cls],type_ignores=[]),'original-pure-job-parser','exec'),env)
c=env['KickTheMapClient'].__new__(env['KickTheMapClient']);c.logged_in_email='';c._set_jobs_diagnostics=lambda **kw:None
records=[dict(Id=12,AboutProject='PS12',address_road='Road',address_number=0,address={'number':4,'postcode':'1234 AB','town':'Town','district':'District','region':'Region','country':'NL'},can_download_cloud=True),dict(id='13',name='PS13',Download='1',address={'road':'Straat','number':'3','postcode':'1000 AA','town':'Dorp','region':'Provincie'}),dict(Id=14,AboutProject='Numeric',Download=.5,coordinates=[1,2],municipality=True),dict(Id=15,AboutProject=' Zero ',x=0,y=0,address_country='NL',can_download_cloud='true',status=-1),dict(Id=16,name='Missing',address=None,status='3',s3_root_dir='new-storage-layout/job16')]
cases=[]
for record in records:
 j=c._parse_job_projects([record])[0]
 keys=['job_id','title','address','municipality','coordinates','client_date','status','prefix']
 expected={k:getattr(j,k) for k in keys};expected['downloadable']=j.download_available
 cases.append(dict(raw={'data':[record]},expected=expected))
inline=[]
for assignment in ['var projects','let projectList','const project_data','window.jobs','window . jobData','var jobs','notprojects','$projects']:
 html=f'<script>{assignment} = [{{"Id":42,"AboutProject":"Quote \\\" [bracket]"}}];</script>'
 try: expected=c._parse_job_projects(json.loads(c._extract_js_value(html,'var projects')))[0].job_id
 except RuntimeError:expected=None
 inline.append(dict(html=html,expected=expected))
for html in ['var projects=null; const job_list=[{"Id":42}]','var projects=[{broken];','no project data','var projects=[];var jobs=[{"Id":42}]']:
 try:
  jobs=c._parse_job_projects(json.loads(c._extract_js_value(html,'var projects')));expected=jobs[0].job_id if jobs else 0
 except RuntimeError:expected=None
 inline.append(dict(html=html,expected=expected))
out=Path(__import__('os').environ.get('SLEUFBASE_LIVE_JOBS_FIXTURE',ROOT/'rust/tests/fixtures/live-jobs.json'));out.write_text(json.dumps(dict(source_sha256=hashlib.sha256(source.encode()).hexdigest(),cases=cases,inline=inline),ensure_ascii=False,indent=2)+'\n',encoding='utf-8');print(f'{len(cases)} original pure job parser + {len(inline)} inline fallback contracts: {out}')
