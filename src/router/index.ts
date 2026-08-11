import type { RouteRecordRaw } from 'vue-router';
import { createRouter, createWebHistory } from 'vue-router';

/**
 * Hub 主界面的路由名称集合。
 * 业务含义：左侧导航、Tauri 事件和页面路由共用同一套路由名称，避免再维护一套视图切换状态。
 */
export const HubRouteName = {
    VoicePolish: 'VoicePolish',
    VoicePolishDictionary: 'VoicePolishDictionary',
    TextPolish: 'TextPolish',
    SessionManage: 'SessionManage',
    TaskManage: 'TaskManage',
    Permission: 'Permission',
    ModelManage: 'ModelManage',
    HttpApiDoc: 'HttpApiDoc',
    Settings: 'Settings'
} as const;

/**
 * Hub 主界面的页面路由配置。
 * 业务含义：每个左侧导航入口都对应独立 URL，刷新、前进后退和外部事件导航都能落到真实页面。
 */
const hubRoutes: RouteRecordRaw[] = [
    {
        path: '/',
        redirect: '/voice-polish'
    },
    {
        path: '/voice-polish/dictionary',
        name: HubRouteName.VoicePolishDictionary,
        component: () => import('@/views/voicePolish/dictionaryList.vue'),
        meta: { title: '词典列表' }
    },
    {
        path: '/voice-polish',
        name: HubRouteName.VoicePolish,
        component: () => import('@/views/voicePolish/index.vue'),
        meta: { title: '语音转文字润色' }
    },
    {
        path: '/text-polish',
        name: HubRouteName.TextPolish,
        component: () => import('@/views/textPolish/index.vue'),
        meta: { title: '润色' }
    },
    {
        path: '/session-manage',
        name: HubRouteName.SessionManage,
        component: () => import('@/views/sessionManage/index.vue'),
        meta: { title: '会话管理' }
    },
    {
        path: '/task-manage',
        name: HubRouteName.TaskManage,
        component: () => import('@/views/taskManage/index.vue'),
        meta: { title: '任务管理' }
    },
    {
        path: '/permission',
        name: HubRouteName.Permission,
        component: () => import('@/views/permission/index.vue'),
        meta: { title: '权限管理' }
    },
    {
        path: '/model-manage',
        name: HubRouteName.ModelManage,
        component: () => import('@/views/modelManage/index.vue'),
        meta: { title: '模型管理' }
    },
    {
        path: '/http-api-doc',
        name: HubRouteName.HttpApiDoc,
        component: () => import('@/views/httpApiDoc/index.vue'),
        meta: { title: 'HTTP API 文档' }
    },
    {
        path: '/settings',
        name: HubRouteName.Settings,
        component: () => import('@/views/settings/index.vue'),
        meta: { title: '系统设置' }
    },
    {
        path: '/:pathMatch(.*)*',
        redirect: '/voice-polish'
    }
];

/**
 * 应用路由实例。
 * 流程：使用 Web History 提供标准 URL，再注册 Hub 主界面的所有页面路由。
 * 返回：Vue Router 插件实例。
 * 边界：未知路径会回到语音润色默认页，避免桌面端或网页预览出现空白页。
 */
export const router = createRouter({
    history: createWebHistory(),
    routes: hubRoutes
});
