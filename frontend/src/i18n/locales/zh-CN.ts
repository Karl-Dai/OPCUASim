export type DictShape = {
  common: {
    confirm: string
    cancel: string
    ok: string
    close: string
    save: string
    refresh: string
    clear: string
    export: string
    delete: string
    add: string
    loading: string
  }
  about: {
    copiedSuffix: string
  }
  appDialog: {
    cancel: string
    ok: string
    titleAlert: string
    titleConfirm: string
    titlePrompt: string
    backendMessageFallback: string
  }
  app: {
    title: string
  }
  serverState: {
    running: string
    starting: string
    stopping: string
    stopped: string
  }
  toolbar: {
    start: string
    stop: string
    endpoint: string
    openProject: string
    saveProject: string
    config: string
    newFolder: string
    folderNameHint: string
    addFolder: string
    newNode: string
    nodeNameHint: string
    dataType: string
    simMode: string
    writable: string
    addNode: string
  }
  statusBar: {
    folders: string
    nodes: string
    endpoint: string
    seq: string
  }
  addressTree: {
    title: string
    emptyTitle: string
    emptyHint: string
    newSubfolder: string
    deleteFolder: string
    deleteNode: string
  }
  nodeTable: {
    title: string
    count: string
    emptyTitle: string
    emptyHint: string
    colName: string
    colNodeId: string
    colDataType: string
    colSimMode: string
    colValue: string
    colRw: string
    remove: string
    removeSelected: string
    selectedCount: string
  }
  propertyEditor: {
    title: string
    emptyTitle: string
    emptyHint: string
    nodeInfo: string
    nodeId: string
    name: string
    parent: string
    dataType: string
    writable: string
    euRange: string
    currentValue: string
    simulation: string
    apply: string
  }
  simulation: {
    static: string
    random: string
    sine: string
    linear: string
    script: string
    value: string
    min: string
    max: string
    intervalMs: string
    amplitude: string
    offset: string
    periodMs: string
    start: string
    step: string
    bounce: string
    repeat: string
    expression: string
  }
  config: {
    title: string
    name: string
    port: string
    endpointUrl: string
    anonymousEnabled: string
    maxSessions: string
    maxSubscriptions: string
    historyBuffer: string
    historyDisabled: string
    eventHistory: string
    securityPolicies: string
    securityModes: string
    save: string
  }
  project: {
    saved: string
    loaded: string
    loadFailed: string
    saveFailed: string
  }
}

const dict: DictShape = {
  common: {
    confirm: '确认',
    cancel: '取消',
    ok: '确定',
    close: '关闭',
    save: '保存',
    refresh: '刷新',
    clear: '清空',
    export: '导出',
    delete: '删除',
    add: '添加',
    loading: '加载中...',
  },
  about: {
    copiedSuffix: '已复制到剪贴板',
  },
  appDialog: {
    cancel: '取消',
    ok: '确定',
    titleAlert: '提示',
    titleConfirm: '确认',
    titlePrompt: '输入',
    backendMessageFallback: '后端操作失败（技术上下文：{technical}）',
  },
  app: {
    title: 'OPCUA Server',
  },
  serverState: {
    running: '运行中',
    starting: '启动中',
    stopping: '停止中',
    stopped: '已停止',
  },
  toolbar: {
    start: '启动',
    stop: '停止',
    endpoint: 'Endpoint',
    openProject: '打开',
    saveProject: '保存',
    config: '配置',
    newFolder: '新建文件夹',
    folderNameHint: '显示名称',
    addFolder: '添加',
    newNode: '新建节点',
    nodeNameHint: '名称',
    dataType: '类型',
    simMode: '仿真',
    writable: 'RW',
    addNode: '添加',
  },
  statusBar: {
    folders: '📁 {count} 文件夹',
    nodes: '📊 {count} 节点',
    endpoint: 'Endpoint',
    seq: 'seq #{seq}',
  },
  addressTree: {
    title: 'ADDRESS SPACE',
    emptyTitle: '地址空间为空',
    emptyHint: '使用顶部 📁 / 📊 添加文件夹与变量',
    newSubfolder: '新建子文件夹',
    deleteFolder: '删除文件夹',
    deleteNode: '删除节点',
  },
  nodeTable: {
    title: '节点列表',
    count: '· {count} 个变量',
    emptyTitle: '尚未定义变量',
    emptyHint: '使用顶部 📊 新建节点 添加一个 Variable',
    colName: 'Name',
    colNodeId: 'NodeId',
    colDataType: 'DataType',
    colSimMode: 'SimMode',
    colValue: 'Value',
    colRw: 'RW',
    remove: '删除节点',
    removeSelected: '移除选中',
    selectedCount: '已选 {count}',
  },
  propertyEditor: {
    title: '节点属性',
    emptyTitle: '未选择节点',
    emptyHint: '从左侧地址空间或节点表中选择一个变量',
    nodeInfo: 'Node Info',
    nodeId: 'NodeId',
    name: 'Name',
    parent: 'Parent',
    dataType: 'DataType',
    writable: 'Writable',
    euRange: 'EU Range',
    currentValue: 'Current Value',
    simulation: 'Simulation',
    apply: '应用',
  },
  simulation: {
    static: 'Static',
    random: 'Random',
    sine: 'Sine',
    linear: 'Linear',
    script: 'Script',
    value: '值',
    min: 'Min',
    max: 'Max',
    intervalMs: '间隔 (ms)',
    amplitude: '振幅',
    offset: '偏移',
    periodMs: '周期 (ms)',
    start: '起点',
    step: '步长',
    bounce: 'Bounce (否则 Repeat)',
    repeat: 'Repeat',
    expression: '表达式',
  },
  config: {
    title: '服务器配置',
    name: '名称',
    port: '端口',
    endpointUrl: 'Endpoint URL',
    anonymousEnabled: '允许匿名访问',
    maxSessions: '最大会话数',
    maxSubscriptions: '每会话最大订阅数',
    historyBuffer: '历史缓冲容量(条/节点)',
    historyDisabled: '(已禁用)',
    eventHistory: '事件历史容量(条/源)',
    securityPolicies: '安全策略',
    securityModes: '安全模式',
    save: '保存',
  },
  project: {
    saved: '项目已保存到 {path}',
    loaded: '项目已加载 ({path})',
    loadFailed: '加载项目失败: {error}',
    saveFailed: '保存项目失败: {error}',
  },
}

export default dict
