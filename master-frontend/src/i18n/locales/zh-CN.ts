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
    remove: string
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
  state: {
    connected: string
    connecting: string
    disconnected: string
    reconnecting: string
  }
  toolbar: {
    connect: string
    disconnect: string
    newConnection: string
    deleteConnection: string
    refresh: string
    saveProject: string
    openProject: string
    discover: string
    newGroup: string
    certManager: string
    confirmDeleteConnection: string
    confirmDeleteGroup: string
    projectSaved: string
    projectSaveFailed: string
    projectLoaded: string
    projectLoadFailed: string
    discoverPrompt: string
    discoverFailed: string
    discoverEmpty: string
    discoverResults: string
    groupPrompt: string
  }
  tree: {
    title: string
    noConnections: string
    browse: string
    monitored: string
    polling: string
    groups: string
    groupNameHint: string
    noGroups: string
    addAllVariables: string
    callMethod: string
    viewHistory: string
    loadingRoot: string
    emptyRoot: string
    selected: string
  }
  dataTable: {
    title: string
    count: string
    searchPlaceholder: string
    selectedCount: string
    removeSelected: string
    remove: string
    addToGroup: string
    noGroups: string
    emptyTitle: string
    emptyHint: string
    colNodeId: string
    colName: string
    colType: string
    colValue: string
    colQuality: string
    colSrcTs: string
    colSrvTs: string
    colMode: string
  }
  valuePanel: {
    title: string
    emptyTitle: string
    emptyHint: string
    noNode: string
    noNodeHint: string
    nodeInfo: string
    currentValue: string
    actions: string
    read: string
    readResult: string
    dataType: string
    accessLevel: string
    value: string
    quality: string
    desc: string
    mode: string
    access: string
    sourceTimestamp: string
    serverTimestamp: string
    write: string
    writeValue: string
    writeSuccess: string
    readFailed: string
    writeFailed: string
    errBoolean: string
    errFloat: string
    errInt: string
    errUint: string
  }
  history: {
    title: string
    mode: string
    modeRaw: string
    modeProcessed: string
    modeEvents: string
    aggType: string
    intervalMs: string
    quick: string
    maxValues: string
    refresh: string
    loading: string
    noData: string
    invalidRange: string
    colTime: string
    colValue: string
    colStatus: string
    colSeverity: string
    colMessage: string
    pointCount: string
    eventCount: string
    emptyTitle: string
    emptyHint: string
  }
  events: {
    title: string
    connection: string
    sourceNode: string
    sourceNodeHint: string
    subscribe: string
    subscribed: string
    unsubscribe: string
    clear: string
    count: string
    emptyTitle: string
    emptyHint: string
    subscribeFailed: string
    colTime: string
    colSeverity: string
    colSource: string
    colMessage: string
  }
  log: {
    title: string
    direction: string
    directionAll: string
    directionRequest: string
    directionResponse: string
    search: string
    searchPlaceholder: string
    autoScroll: string
    clear: string
    export: string
    exporting: string
    exportFailed: string
    refresh: string
    colTime: string
    colDirection: string
    colService: string
    colDetail: string
    colStatus: string
    noConnection: string
    noLogs: string
    noMatches: string
    filteredCount: string
    backendDetailFallback: string
    connection: {
      connecting: string
      connected: string
      disconnected: string
      reconnecting: string
    }
  }
  newConn: {
    title: string
    name: string
    nameHint: string
    endpointUrl: string
    securityPolicy: string
    securityMode: string
    auth: string
    authAnonymous: string
    authUserPassword: string
    authCertificate: string
    username: string
    password: string
    certPath: string
    keyPath: string
    timeoutMs: string
    discover: string
    discovering: string
    discovered: string
    noEndpoints: string
    create: string
    nameRequired: string
    urlRequired: string
    urlInvalid: string
    usernameRequired: string
    certPathsRequired: string
    createFailed: string
    created: string
  }
  methodCall: {
    title: string
    method: string
    object: string
    inputs: string
    noInputs: string
    loadingArgs: string
    outputs: string
    notExecuted: string
    execute: string
    executing: string
    close: string
    callFailed: string
    argsFailed: string
    status: string
  }
  cert: {
    title: string
    pkiDir: string
    refresh: string
    trusted: string
    rejected: string
    certCount: string
    file: string
    issuer: string
    thumbprint: string
    validity: string
    moveToTrusted: string
    moveToRejected: string
    delete: string
    close: string
    loadFailed: string
    moveFailed: string
    deleteFailed: string
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
    remove: '移除',
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
    title: 'OPCUA Master',
  },
  state: {
    connected: '已连接',
    connecting: '连接中',
    disconnected: '未连接',
    reconnecting: '重连中',
  },
  toolbar: {
    connect: '连接',
    disconnect: '断开',
    newConnection: '新建连接',
    deleteConnection: '删除',
    refresh: '刷新',
    saveProject: '保存',
    openProject: '打开',
    discover: '发现端点',
    newGroup: '新建分组',
    certManager: '证书',
    confirmDeleteConnection: '确定要删除选中的连接吗？',
    confirmDeleteGroup: '确定要删除该分组吗？',
    projectSaved: '项目已保存到 {path}',
    projectSaveFailed: '保存项目失败：{error}',
    projectLoaded: '项目已从 {path} 加载',
    projectLoadFailed: '加载项目失败：{error}',
    discoverPrompt: '输入要发现的端点 URL（opc.tcp://...）',
    discoverFailed: '发现端点失败：{error}',
    discoverEmpty: '未发现任何端点',
    discoverResults: '发现 {count} 个端点',
    groupPrompt: '分组名称',
  },
  tree: {
    title: '连接',
    noConnections: '暂无连接',
    browse: '地址空间',
    monitored: '监控节点',
    polling: '轮询节点',
    groups: '分组',
    groupNameHint: '分组名称',
    noGroups: '(暂无分组)',
    addAllVariables: '添加此节点下所有变量',
    callMethod: '调用方法...',
    viewHistory: '查看历史',
    loadingRoot: '加载根节点…',
    emptyRoot: '无可用根节点',
    selected: '已选 {count} 个变量',
  },
  dataTable: {
    title: '监控数据',
    count: '{count} 个节点',
    searchPlaceholder: 'NodeId / Name / Value',
    selectedCount: '已选 {count}',
    removeSelected: '移除选中',
    remove: '移除',
    addToGroup: '加入分组',
    noGroups: '暂无分组',
    emptyTitle: '尚无监控节点',
    emptyHint: '在左侧地址空间勾选变量后添加',
    colNodeId: 'NodeId',
    colName: '名称',
    colType: '类型',
    colValue: '值',
    colQuality: '质量',
    colSrcTs: '源时间',
    colSrvTs: '服务器时间',
    colMode: '模式',
  },
  valuePanel: {
    title: '节点详情',
    emptyTitle: '未选择节点',
    emptyHint: '从左侧树或中央表格选择一个节点',
    noNode: '未选择节点',
    noNodeHint: '从中央表格选择一行查看详情',
    nodeInfo: '节点信息',
    currentValue: '当前值',
    actions: '操作',
    read: '读取',
    readResult: '读取结果',
    dataType: '数据类型',
    accessLevel: '访问级别',
    value: '值',
    quality: '质量',
    desc: '描述',
    mode: '模式',
    access: '访问',
    sourceTimestamp: '源时间戳',
    serverTimestamp: '服务器时间戳',
    write: '写入值',
    writeValue: '写入',
    writeSuccess: '写入成功 ({nodeId})',
    readFailed: '读取失败：{error}',
    writeFailed: '写入失败：{error}',
    errBoolean: '需要 true/false 或 0/1',
    errFloat: '需要浮点数',
    errInt: '需要整数',
    errUint: '需要非负整数',
  },
  history: {
    title: '历史趋势',
    mode: '模式',
    modeRaw: '原始',
    modeProcessed: '聚合',
    modeEvents: '事件',
    aggType: '聚合函数',
    intervalMs: '间隔(ms)',
    quick: '快捷',
    maxValues: '最多',
    refresh: '刷新',
    loading: '加载中…',
    noData: '暂无历史数据',
    invalidRange: '起始时间必须早于结束时间且格式有效',
    colTime: '源时间戳',
    colValue: '值',
    colStatus: '状态',
    colSeverity: '严重性',
    colMessage: '消息',
    pointCount: '{count} 个点',
    eventCount: '{count} 个事件',
    emptyTitle: '未选择历史节点',
    emptyHint: '从地址空间或监控表格右键「查看历史」',
  },
  events: {
    title: '事件订阅',
    connection: '连接',
    sourceNode: '源节点',
    sourceNodeHint: '例如 ns=2;i=1',
    subscribe: '订阅',
    subscribed: '已订阅',
    unsubscribe: '取消',
    clear: '清空',
    count: '共 {count} 条',
    emptyTitle: '暂无事件',
    emptyHint: '输入源节点并点击订阅',
    subscribeFailed: '订阅失败：{error}',
    colTime: '时间',
    colSeverity: '严重性',
    colSource: '来源',
    colMessage: '消息',
  },
  log: {
    title: '通信日志',
    direction: '方向',
    directionAll: '全部',
    directionRequest: '请求',
    directionResponse: '响应',
    search: '搜索',
    searchPlaceholder: 'Service / Detail',
    autoScroll: '自动滚动',
    clear: '清空',
    export: '导出 CSV',
    exporting: '导出中…',
    exportFailed: '导出失败：{error}',
    refresh: '刷新',
    colTime: '时间',
    colDirection: '方向',
    colService: '服务',
    colDetail: '详情',
    colStatus: '状态',
    noConnection: '未选择连接',
    noLogs: '暂无日志',
    noMatches: '无匹配日志',
    filteredCount: '{visible}/{total}',
    backendDetailFallback: '后端详情（技术上下文：{technical}）',
    connection: {
      connecting: '正在连接 {endpoint_url}',
      connected: '已连接 {endpoint_url}',
      disconnected: '已断开 {endpoint_url}',
      reconnecting: '正在重连 {endpoint_url}',
    },
  },
  newConn: {
    title: '新建连接',
    name: '名称',
    nameHint: '连接显示名称',
    endpointUrl: 'Endpoint URL',
    securityPolicy: 'Security Policy',
    securityMode: 'Security Mode',
    auth: '认证方式',
    authAnonymous: 'Anonymous',
    authUserPassword: 'UserPassword',
    authCertificate: 'Certificate',
    username: '用户名',
    password: '密码',
    certPath: '证书路径',
    keyPath: '私钥路径',
    timeoutMs: '超时 (ms)',
    discover: '发现',
    discovering: '发现中…',
    discovered: '发现的端点 ({count})',
    noEndpoints: '未发现端点',
    create: '创建',
    nameRequired: '连接名不能为空',
    urlRequired: 'Endpoint URL 不能为空',
    urlInvalid: 'Endpoint URL 必须以 opc.tcp:// 开头',
    usernameRequired: '用户名不能为空',
    certPathsRequired: '证书与私钥路径都不能为空',
    createFailed: '创建连接失败：{error}',
    created: '已创建连接 {name}',
  },
  methodCall: {
    title: '调用方法: {name}',
    method: 'Method',
    object: 'Object',
    inputs: '输入参数',
    noInputs: '(无入参)',
    loadingArgs: '加载参数...',
    outputs: '输出参数',
    notExecuted: '(尚未执行)',
    execute: '执行',
    executing: '执行中…',
    close: '关闭',
    callFailed: '调用失败：{error}',
    argsFailed: '读取参数失败：{error}',
    status: 'Status: {status}',
  },
  cert: {
    title: '证书管理',
    pkiDir: 'PKI 目录',
    refresh: '刷新',
    trusted: '信任',
    rejected: '拒绝',
    certCount: '{count} 证书',
    file: '文件',
    issuer: 'Issuer',
    thumbprint: 'Thumbprint',
    validity: '有效期',
    moveToTrusted: '→ 信任',
    moveToRejected: '→ 拒绝',
    delete: '删除',
    close: '关闭',
    loadFailed: '加载证书失败：{error}',
    moveFailed: '移动证书失败：{error}',
    deleteFailed: '删除证书失败：{error}',
  },
}

export default dict
