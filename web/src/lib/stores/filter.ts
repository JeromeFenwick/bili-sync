import { writable } from 'svelte/store';
import type { VideoSortBy, VideoSortOrder } from '$lib/types';

export type StatusFilterValue = 'failed' | 'succeeded' | 'waiting' | 'skipped' | 'paid' | null;

export interface AppState {
	query: string;
	currentPage: number;
	videoSource: {
		type: string;
		id: string;
	} | null;
	statusFilter: StatusFilterValue | null;
	sortBy: VideoSortBy;
	sortOrder: VideoSortOrder;
}

export const appStateStore = writable<AppState>({
	query: '',
	currentPage: 0,
	videoSource: null,
	statusFilter: null,
	// 默认按下载时间倒序
	sortBy: 'download_time',
	sortOrder: 'desc'
});

export const ToQuery = (state: AppState): string => {
	const { query, videoSource, currentPage, statusFilter, sortBy, sortOrder } = state;
	const params = new URLSearchParams();
	if (currentPage > 0) {
		params.set('page', String(currentPage));
	}
	if (query.trim()) {
		params.set('query', query);
	}
	if (videoSource && videoSource.type && videoSource.id) {
		params.set(videoSource.type, videoSource.id);
	}
	if (statusFilter) {
		params.set('status_filter', statusFilter);
	}
	if (sortBy) {
		params.set('sort_by', sortBy);
	}
	if (sortOrder) {
		params.set('sort_order', sortOrder);
	}
	const queryString = params.toString();
	return queryString ? `videos?${queryString}` : 'videos';
};

// 将 AppState 转换为请求体中的筛选参数
export const ToFilterParams = (
	state: AppState
): {
	query?: string;
	collection?: number;
	favorite?: number;
	submission?: number;
	watch_later?: number;
	status_filter?: Exclude<StatusFilterValue, null>;
} => {
	const params: {
		query?: string;
		collection?: number;
		favorite?: number;
		submission?: number;
		watch_later?: number;
		status_filter?: Exclude<StatusFilterValue, null>;
	} = {};

	if (state.query.trim()) {
		params.query = state.query;
	}

	if (state.videoSource && state.videoSource.type && state.videoSource.id) {
		const { type, id } = state.videoSource;
		params[type as 'collection' | 'favorite' | 'submission' | 'watch_later'] = parseInt(id);
	}
	if (state.statusFilter) {
		params.status_filter = state.statusFilter;
	}
	return params;
};

// 检查是否有活动的筛选条件
export const hasActiveFilters = (state: AppState): boolean => {
	return !!(state.query.trim() || state.videoSource || state.statusFilter);
};

export const setQuery = (query: string) => {
	appStateStore.update((state) => ({
		...state,
		query
	}));
};

export const setCurrentPage = (page: number) => {
	appStateStore.update((state) => ({
		...state,
		currentPage: page
	}));
};

export const setStatusFilter = (statusFilter: StatusFilterValue | null) => {
	appStateStore.update((state) => ({
		...state,
		statusFilter
	}));
};

export const resetCurrentPage = () => {
	appStateStore.update((state) => ({
		...state,
		currentPage: 0
	}));
};

export const setAll = (
	query: string,
	currentPage: number,
	videoSource: { type: string; id: string } | null,
	statusFilter: StatusFilterValue | null,
	sortBy: VideoSortBy,
	sortOrder: VideoSortOrder
) => {
	appStateStore.set({
		query,
		currentPage,
		videoSource,
		statusFilter,
		sortBy,
		sortOrder
	});
};
