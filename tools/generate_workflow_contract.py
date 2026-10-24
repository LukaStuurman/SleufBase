"""Capture the real original menu in fake widgets; never initialize the app.

Only the hash-validated _build_menu code object is invoked. Callbacks are names,
not executable app methods. Source AST records subsequent menu additions and
the original DXF viewport primitive set. This is inventory, not acceptance.
"""
from pathlib import Path
from types import CodeType,FunctionType,SimpleNamespace
import ast,importlib.util,json,os
ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ.get('SLEUFBASE_WORKFLOW_CONTRACT',ROOT/'rust/tests/fixtures/workflows/original.json'))

class Menu:
    def __init__(self,*args,**kwargs):self.items=[]
    def add_command(self,**kwargs):self.items.append(dict(kind='command',label=kwargs['label'],action=kwargs['command']))
    def add_separator(self):self.items.append(dict(kind='separator'))
    def add_cascade(self,**kwargs):self.items.append(dict(kind='menu',label=kwargs['label'],items=kwargs['menu'].items))
class Workspace:
    def __getattr__(self,name):return name
    def config(self,**kwargs):self.menu=kwargs['menu']

def main():
    spec=importlib.util.spec_from_file_location('workflow_legacy_helper',ROOT/'legacy_bytecode.py')
    helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper)
    code=helper.read_validated_code('app',ROOT/'legacy_bytecode.py')
    cls=next(c for c in code.co_consts if isinstance(c,CodeType) and c.co_name=='KlicViewerApp')
    method=next(c for c in cls.co_consts if isinstance(c,CodeType) and c.co_name=='_build_menu')
    app=Workspace();FunctionType(method,{'tk':SimpleNamespace(Menu=Menu)})(app)
    app_tree=ast.parse((ROOT/'app.py').read_text(encoding='utf-8'))
    patches=[]
    for fn in ast.walk(app_tree):
        if not isinstance(fn,ast.FunctionDef) or not fn.name.startswith('_build_menu_with_'):continue
        commands=[]
        for node in ast.walk(fn):
            if isinstance(node,ast.Call) and isinstance(node.func,ast.Attribute) and node.func.attr in ('add_command','insert_command'):
                values={kw.arg:ast.unparse(kw.value) for kw in node.keywords if kw.arg in ('label','command')}
                if 'label' in values:commands.append(values)
        if commands:patches.append(dict(function=fn.name,commands=commands))
    tree=ast.parse((ROOT/'dxf.py').read_text(encoding='utf-8'))
    fn=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='_polyline_from_entity')
    primitives=[]
    for node in ast.walk(fn):
        if not isinstance(node,ast.Compare) or not isinstance(node.left,ast.Name) or node.left.id!='entity_type':continue
        value=node.comparators[0]
        if isinstance(value,ast.Constant):primitives.append(value.value)
        elif isinstance(value,(ast.Set,ast.Tuple)):primitives.extend(n.value for n in value.elts)
    result=dict(base_menu=app.menu.items,source_menu_patches=patches,dxf_viewport_primitives=primitives)
    OUT.parent.mkdir(parents=True,exist_ok=True)
    OUT.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(f'Original base menu / {len(patches)} source menu hooks / {len(primitives)} DXF viewport primitive types captured without app initialization')

if __name__=='__main__':main()
