<script lang="ts">
	import VideoCard from '$lib/components/video-card.svelte';
	import Pagination from '$lib/components/pagination.svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
	import EditIcon from '@lucide/svelte/icons/edit';
	import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
	import ChevronLeftIcon from '@lucide/svelte/icons/chevron-left';
	import api from '$lib/api';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import type {
		VideosResponse,
		VideoSourcesResponse,
		ApiError,
		VideoSource,
		UpdateFilteredVideoStatusRequest,
		VideoSortBy,
		VideoSortOrder
	} from '$lib/types';
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { VIDEO_SOURCES } from '$lib/consts';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';
	import {
		appStateStore,
		resetCurrentPage,
		setAll,
		setCurrentPage,
		setQuery,
		setStatusFilter,
		ToQuery,
		ToFilterParams,
		hasActiveFilters,
		type StatusFilterValue
	} from '$lib/stores/filter';
	import { toast } from 'svelte-sonner';
	import DropdownFilter, { type Filter } from '$lib/components/dropdown-filter.svelte';
	import SearchBar from '$lib/components/search-bar.svelte';
	import FilteredStatusEditor from '$lib/components/filtered-status-editor.svelte';
	import BatchStatusEditor from '$lib/components/batch-status-editor.svelte';
	import StatusFilter from '$lib/components/status-filter.svelte';

	const pageSize = 20;

	let videosData = $state<VideosResponse | null>(null);
	let loading = $state(false);

	let lastSearch: string | null = null;

	let resetAllDialogOpen = $state(false);
	let resettingAll = $state(false);

	let forceReset = $state(false);

	let updateAllDialogOpen = $state(false);
	let updatingAll = $state(false);

	// 批量选择相关
	let isSelectionMode = $state(false); // 是否进入选择模式
	let selectedVideoIds = $state<Set<number>>(new Set());
	let batchEditDialogOpen = $state(false);
	let batchEditing = $state(false);

	let videoSources: VideoSourcesResponse | null = null;
	let filters: Record<string, Filter> | null = null;
const SORT_OPTIONS: { value: VideoSortBy; label: string }[] = [
	{ value: 'download_time', label: '按下载时间' },
	{ value: 'subscribe_time', label: '按订阅时间' },
	{ value: 'publish_time', label: '按投稿时间' }
];

	function getApiParams(searchParams: URLSearchParams) {
		let videoSource = null;
		for (const source of Object.values(VIDEO_SOURCES)) {
			const value = searchParams.get(source.type);
			if (value) {
				videoSource = { type: source.type, id: value };
			}
		}
		// 支持从 URL 里还原状态筛选
		const statusFilterParam = searchParams.get('status_filter');
		const statusFilter: StatusFilterValue | null =
			statusFilterParam === 'failed' ||
			statusFilterParam === 'succeeded' ||
			statusFilterParam === 'waiting' ||
			statusFilterParam === 'skipped' ||
			statusFilterParam === 'paid'
				? statusFilterParam
				: null;
		const hasSourceFilter = !!videoSource;

		let sortBy = searchParams.get('sort_by') as VideoSortBy | null;
		let sortOrder = searchParams.get('sort_order') as VideoSortOrder | null;

		if (!sortBy) {
			sortBy = hasSourceFilter ? 'subscribe_time' : 'download_time';
		}
		if (!sortOrder) {
			sortOrder = 'desc';
		}

		return {
			query: searchParams.get('query') || '',
			videoSource,
			statusFilter,
			pageNum: parseInt(searchParams.get('page') || '0'),
			sortBy,
			sortOrder
		};
	}

	async function loadVideos(
		query: string,
		pageNum: number = 0,
		filter?: { type: string; id: string } | null,
		statusFilter: StatusFilterValue | null = null,
		sortBy: VideoSortBy = 'download_time',
		sortOrder: VideoSortOrder = 'desc'
	) {
		loading = true;
		try {
			const params: Record<string, string | number | boolean> = {
				page: pageNum,
				page_size: pageSize
			};
			if (query) {
				params.query = query;
			}
			if (filter) {
				params[filter.type] = parseInt(filter.id);
			}
			if (statusFilter) {
				params.status_filter = statusFilter;
			}
			if (sortBy) {
				params.sort_by = sortBy;
			}
			if (sortOrder) {
				params.sort_order = sortOrder;
			}
			console.log('loadVideos: params =', params);
			const result = await api.getVideos(params);
			videosData = result.data;
			console.log('loadVideos: result.data =', result.data);
		} catch (error) {
			console.error('加载视频失败：', error);
			toast.error('加载视频失败', {
				description: (error as ApiError).message
			});
		} finally {
			loading = false;
		}
	}

	async function handlePageChange(pageNum: number) {
		setCurrentPage(pageNum);
		goto(`/${ToQuery($appStateStore)}`);
	}

	async function handleSearchParamsChange(searchParams: URLSearchParams) {
		const { query, videoSource, pageNum, statusFilter, sortBy, sortOrder } =
			getApiParams(searchParams);
		setAll(query, pageNum, videoSource, statusFilter, sortBy, sortOrder);
		await loadVideos(query, pageNum, videoSource, statusFilter, sortBy, sortOrder);
	}

	async function handleResetVideo(id: number, forceReset: boolean) {
		try {
			const result = await api.resetVideoStatus(id, { force: forceReset });
			const data = result.data;
			if (data.resetted) {
				toast.success('重置成功', {
					description: `视频「${data.video.name}」已重置`
				});
				const { query, currentPage, videoSource, statusFilter, sortBy, sortOrder } =
					$appStateStore;
				await loadVideos(query, currentPage, videoSource, statusFilter, sortBy, sortOrder);
			} else {
				toast.info('重置无效', {
					description: `视频「${data.video.name}」没有失败的状态，无需重置`
				});
			}
		} catch (error) {
			console.error('重置失败：', error);
			toast.error('重置失败', {
				description: (error as ApiError).message
			});
		}
	}

	async function handleClearAndResetVideo(id: number) {
		try {
			const result = await api.clearAndResetVideoStatus(id);
			const data = result.data;
			if (data.warning) {
				toast.warning('清空重置成功', {
					description: data.warning
				});
			} else {
				toast.success('清空重置成功', {
					description: `视频「${data.video.name}」已清空重置`
				});
			}
			const { query, currentPage, videoSource, statusFilter, sortBy, sortOrder } =
				$appStateStore;
			await loadVideos(query, currentPage, videoSource, statusFilter, sortBy, sortOrder);
		} catch (error) {
			console.error('清空重置失败：', error);
			toast.error('清空重置失败', {
				description: (error as ApiError).message
			});
		}
	}

	async function handleResetAllVideos() {
		resettingAll = true;
		try {
			// 获取筛选参数
			const filterParams = ToFilterParams($appStateStore);
			const result = await api.resetFilteredVideoStatus({
				...filterParams,
				force: forceReset
			});
			const data = result.data;
			if (data.resetted) {
				toast.success('重置成功', {
					description: `已重置 ${data.resetted_videos_count} 个视频和 ${data.resetted_pages_count} 个分页`
				});
				const { query, currentPage, videoSource, statusFilter, sortBy, sortOrder } =
					$appStateStore;
				await loadVideos(query, currentPage, videoSource, statusFilter, sortBy, sortOrder);
			} else {
				toast.info('没有需要重置的视频');
			}
		} catch (error) {
			console.error('重置失败：', error);
			toast.error('重置失败', {
				description: (error as ApiError).message
			});
		} finally {
			resettingAll = false;
			resetAllDialogOpen = false;
		}
	}

	async function handleUpdateAllVideos(request: UpdateFilteredVideoStatusRequest) {
		updatingAll = true;
		try {
			// 获取筛选参数并合并
			const filterParams = ToFilterParams($appStateStore);
			const fullRequest = {
				...filterParams,
				...request
			};
			const result = await api.updateFilteredVideoStatus(fullRequest);
			const data = result.data;
			if (data.success) {
				toast.success('更新成功', {
					description: `已更新 ${data.updated_videos_count} 个视频和 ${data.updated_pages_count} 个分页`
				});
				const { query, currentPage, videoSource, statusFilter, sortBy, sortOrder } =
					$appStateStore;
				await loadVideos(query, currentPage, videoSource, statusFilter, sortBy, sortOrder);
			} else {
				toast.info('没有视频被更新');
			}
		} catch (error) {
			console.error('更新失败：', error);
			toast.error('更新失败', {
				description: (error as ApiError).message
			});
		} finally {
			updatingAll = false;
			updateAllDialogOpen = false;
		}
	}

	// 批量编辑选中的视频
	async function handleBatchEditVideos(request: UpdateFilteredVideoStatusRequest) {
		if (selectedVideoIds.size === 0) {
			toast.info('请先选择要编辑的视频');
			return;
		}
		batchEditing = true;
		try {
			const fullRequest = {
				...request,
				video_ids: Array.from(selectedVideoIds)
			};
			const result = await api.updateFilteredVideoStatus(fullRequest);
			const data = result.data;
			if (data.success) {
				toast.success('批量更新成功', {
					description: `已更新 ${data.updated_videos_count} 个视频和 ${data.updated_pages_count} 个分页`
				});
			// 清空选择
			selectedVideoIds = new Set();
			const { query, currentPage, videoSource, statusFilter, sortBy, sortOrder } =
				$appStateStore;
			await loadVideos(query, currentPage, videoSource, statusFilter, sortBy, sortOrder);
			// 退出批量编辑模式
			exitBatchEditMode();
			} else {
				toast.info('没有视频被更新');
			}
		} catch (error) {
			console.error('批量更新失败：', error);
			toast.error('批量更新失败', {
				description: (error as ApiError).message
			});
		} finally {
			batchEditing = false;
			batchEditDialogOpen = false;
		}
	}

	// 进入批量编辑模式
	function enterBatchEditMode() {
		isSelectionMode = true;
		selectedVideoIds = new Set();
	}

	// 退出批量编辑模式
	function exitBatchEditMode() {
		isSelectionMode = false;
		selectedVideoIds = new Set();
	}

	// 全选/取消全选（仅针对当前页）
	function toggleSelectAll() {
		if (!videosData) return;
		const currentPageVideoIds = new Set(videosData.videos.map((v) => v.id));
		const allCurrentPageSelected = videosData.videos.every((v) => selectedVideoIds.has(v.id));
		
		if (allCurrentPageSelected) {
			// 取消全选当前页 - 只移除当前页的视频，保留其他页的选择
			const newSet = new Set(selectedVideoIds);
			currentPageVideoIds.forEach((id) => newSet.delete(id));
			selectedVideoIds = newSet;
		} else {
			// 全选当前页 - 添加当前页的所有视频，保留其他页的选择
			const newSet = new Set(selectedVideoIds);
			currentPageVideoIds.forEach((id) => newSet.add(id));
			selectedVideoIds = newSet;
		}
	}

	// 切换单个视频的选择状态
	function toggleVideoSelection(videoId: number) {
		const newSet = new Set(selectedVideoIds);
		if (newSet.has(videoId)) {
			newSet.delete(videoId);
		} else {
			newSet.add(videoId);
		}
		selectedVideoIds = newSet;
	}

	let hasSelectedVideos = $derived(selectedVideoIds.size > 0);
	// 检查当前页的所有视频是否都被选中
	let isAllSelected = $derived(
		videosData && videosData.videos.length > 0 && videosData.videos.every((v) => selectedVideoIds.has(v.id))
	);

	// 获取筛选条件的显示数组
	function getFilterDescriptionParts(): string[] {
		const state = $appStateStore;
		const parts: string[] = [];
		if (state.query.trim()) {
			parts.push(`搜索词："${state.query}"`);
		}
		if (state.videoSource && videoSources) {
			const sourceType = state.videoSource.type;
			const sourceId = parseInt(state.videoSource.id);
			const sourceConfig = Object.values(VIDEO_SOURCES).find((s) => s.type === sourceType);
			if (sourceConfig) {
				const sourceList = videoSources[sourceType as keyof VideoSourcesResponse] as VideoSource[];
				const source = sourceList.find((s) => s.id === sourceId);
				if (source) {
					parts.push(`${sourceConfig.title}：${source.name}`);
				}
			}
		}
		if (state.statusFilter) {
			const statusLabels = {
				failed: '仅失败',
				succeeded: '仅成功',
				waiting: '仅等待',
				skipped: '仅跳过',
				paid: '收费视频'
			};
			parts.push(`状态：${statusLabels[state.statusFilter]}`);
		}
		return parts;
	}

	$effect(() => {
		const currentSearch = $page.url.search;
		// 只在 lastSearch 已设置且发生变化时触发（避免初始化时重复加载）
		if (lastSearch !== null && currentSearch !== lastSearch) {
			lastSearch = currentSearch;
			handleSearchParamsChange($page.url.searchParams);
		}
	});

	$effect(() => {
		if (videoSources) {
			filters = Object.fromEntries(
				Object.values(VIDEO_SOURCES).map((source) => [
					source.type,
					{
						name: source.title,
						icon: source.icon,
						values: Object.fromEntries(
							(videoSources![source.type as keyof VideoSourcesResponse] as VideoSource[]).map(
								(item) => [item.id, item.name]
							)
						)
					}
				])
			);
		} else {
			filters = null;
		}
	});

	onMount(async () => {
		setBreadcrumb([
			{
				label: '视频'
			}
		]);
		videoSources = (await api.getVideoSources()).data;
		// 初始化时直接加载视频
		const currentSearch = $page.url.search;
		console.log('onMount: currentSearch =', currentSearch);
		lastSearch = currentSearch;
		await handleSearchParamsChange($page.url.searchParams);
		console.log('onMount: videosData =', videosData);
	});

	let totalPages = $derived(videosData ? Math.ceil(videosData.total_count / pageSize) : 0);
	let hasFilters = $derived(hasActiveFilters($appStateStore));
	let filterDescriptionParts = $derived(
		videoSources && $appStateStore ? getFilterDescriptionParts() : null
	);

	// 判断是否需要显示返回按钮
	let backButtonInfo = $derived(() => {
		const searchParams = $page.url.searchParams;
		const favorite = searchParams.get('favorite');
		const collection = searchParams.get('collection');
		const submission = searchParams.get('submission');
		
		if (favorite) {
			return { show: true, href: '/me/favorites', label: '返回收藏夹' };
		} else if (collection) {
			return { show: true, href: '/me/collections', label: '返回合集' };
		} else if (submission) {
			return { show: true, href: '/me/uppers', label: '返回UP主' };
		}
		return { show: false, href: '', label: '' };
	});
</script>

<svelte:head>
	<title>主页 - Bili Sync</title>
</svelte:head>

<div class="mb-4 flex items-center justify-between">
	<div class="flex items-center gap-3">
		{#if backButtonInfo().show}
			<Button
				variant="outline"
				size="sm"
				class="shrink-0 cursor-pointer"
				onclick={() => goto(backButtonInfo().href)}
			>
				<ChevronLeftIcon class="mr-1 h-4 w-4" />
				{backButtonInfo().label}
			</Button>
		{/if}
		<SearchBar
			placeholder="搜索视频标题或 BV 号.."
		value={$appStateStore.query}
		onSearch={(value) => {
			setQuery(value);
			resetCurrentPage();
			goto(`/${ToQuery($appStateStore)}`);
		}}
		></SearchBar>
	</div>
	<div class="flex items-center gap-3">
		<!-- 状态筛选 -->
		<div class="flex items-center gap-1">
			<span class="text-muted-foreground text-xs">状态:</span>
			<StatusFilter
				value={$appStateStore.statusFilter}
				onSelect={(value) => {
					setStatusFilter(value);
					resetCurrentPage();
					goto(`/${ToQuery($appStateStore)}`);
				}}
				onRemove={() => {
					setStatusFilter(null);
					resetCurrentPage();
					goto(`/${ToQuery($appStateStore)}`);
				}}
			/>
		</div>
		<!-- 视频源筛选 -->
		<div class="flex items-center gap-1">
			<span class="text-muted-foreground text-xs">来源:</span>
			<DropdownFilter
				{filters}
				selectedLabel={$appStateStore.videoSource}
				onSelect={(type, id) => {
					setAll('', 0, { type, id }, $appStateStore.statusFilter, $appStateStore.sortBy, $appStateStore.sortOrder);
					goto(`/${ToQuery($appStateStore)}`);
				}}
				onRemove={() => {
					setAll('', 0, null, $appStateStore.statusFilter, $appStateStore.sortBy, $appStateStore.sortOrder);
					goto(`/${ToQuery($appStateStore)}`);
				}}
			/>
		</div>
		<!-- 排序选择 -->
		<div class="flex items-center gap-1">
			<span class="text-muted-foreground text-xs">排序:</span>
			<select
				class="border-input bg-background text-foreground hover:bg-accent hover:text-accent-foreground h-8 cursor-pointer rounded-md border px-2 text-xs outline-none"
				bind:value={$appStateStore.sortBy}
				onchange={() => {
					resetCurrentPage();
					goto(`/${ToQuery($appStateStore)}`);
				}}
			>
				{#each SORT_OPTIONS as option}
					<option value={option.value}>{option.label}</option>
				{/each}
			</select>
			<Button
				size="icon"
				variant="outline"
				class="hover:bg-accent hover:text-accent-foreground h-8 w-8"
				onclick={() => {
					appStateStore.update((state) => ({
						...state,
						sortOrder: state.sortOrder === 'desc' ? 'asc' : 'desc',
						currentPage: 0
					}));
					goto(`/${ToQuery($appStateStore)}`);
				}}
			>
				<span class="text-xs">
					{$appStateStore.sortOrder === 'desc' ? '↓' : '↑'}
				</span>
			</Button>
		</div>
	</div>
</div>

{#if videosData}
	<div class="mb-6 flex items-center justify-between">
		<div class="flex items-center gap-6">
			<div class=" text-sm font-medium">
				共 {videosData.total_count} 个视频
			</div>
			<div class=" text-sm font-medium">
				当前第 {$appStateStore.currentPage + 1} / {totalPages} 页
			</div>
			{#if isSelectionMode && hasSelectedVideos}
				<div class=" text-sm font-medium text-blue-600">
					已选择 {selectedVideoIds.size} 个视频
				</div>
			{/if}
		</div>
		<div class="flex items-center gap-2">
			{#if isSelectionMode}
				{#if hasSelectedVideos}
					<Button
						size="sm"
						variant="default"
						class="hover:bg-accent hover:text-accent-foreground h-8 cursor-pointer text-xs font-medium"
						onclick={() => (batchEditDialogOpen = true)}
						disabled={batchEditing || loading}
					>
						<EditIcon class="mr-1.5 h-3 w-3" />
						批量编辑 ({selectedVideoIds.size})
					</Button>
					<Button
						size="sm"
						variant="outline"
						class="hover:bg-accent hover:text-accent-foreground h-8 cursor-pointer text-xs font-medium"
						onclick={() => {
							selectedVideoIds = new Set();
						}}
						disabled={batchEditing || loading}
					>
						取消选择
					</Button>
				{/if}
				<Button
					size="sm"
					variant="outline"
					class="hover:bg-accent hover:text-accent-foreground h-8 cursor-pointer text-xs font-medium"
					onclick={exitBatchEditMode}
					disabled={batchEditing || loading}
				>
					退出批量编辑
				</Button>
			{:else}
				<Button
					size="sm"
					variant="outline"
					class="hover:bg-accent hover:text-accent-foreground h-8 cursor-pointer text-xs font-medium"
					onclick={enterBatchEditMode}
					disabled={loading}
				>
					<EditIcon class="mr-1.5 h-3 w-3" />
					批量编辑
				</Button>
			{/if}
			<Button
				size="sm"
				variant="outline"
				class="hover:bg-accent hover:text-accent-foreground h-8 cursor-pointer text-xs font-medium"
				onclick={() => (updateAllDialogOpen = true)}
				disabled={updatingAll || loading}
			>
				<EditIcon class="mr-1.5 h-3 w-3" />
				{hasFilters ? '编辑筛选' : '编辑全部'}
			</Button>
			<Button
				size="sm"
				variant="outline"
				class="hover:bg-accent hover:text-accent-foreground h-8 cursor-pointer text-xs font-medium"
				onclick={() => (resetAllDialogOpen = true)}
				disabled={resettingAll || loading}
			>
				<RotateCcwIcon class="mr-1.5 h-3 w-3 {resettingAll ? 'animate-spin' : ''}" />
				{hasFilters ? '重置筛选' : '重置全部'}
			</Button>
		</div>
	</div>
{/if}

{#if loading}
	<div class="flex items-center justify-center py-16">
		<div class="text-muted-foreground/70 text-sm">加载中...</div>
	</div>
{:else if videosData?.videos.length}
	{#if isSelectionMode}
		<div class="mb-4 flex items-center gap-2 border-b pb-2">
			<Checkbox
				id="select-all"
				checked={isAllSelected}
				onCheckedChange={toggleSelectAll}
			/>
			<Label for="select-all" class="cursor-pointer text-sm font-medium">
				{isAllSelected ? '取消全选' : '全选'}
			</Label>
		</div>
	{/if}
	<div
		class="mb-8 grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 2xl:grid-cols-5"
	>
		{#each videosData.videos as video (video.id)}
			<div class="relative">
				{#if isSelectionMode}
					<div class="absolute left-2 top-2 z-10" onclick={(e) => e.stopPropagation()}>
						<Checkbox
							checked={selectedVideoIds.has(video.id)}
							onCheckedChange={() => toggleVideoSelection(video.id)}
							class="bg-white/90 backdrop-blur-sm"
						/>
					</div>
				{/if}
				<VideoCard
					{video}
					{isSelectionMode}
					isSelected={selectedVideoIds.has(video.id)}
					onToggleSelection={() => toggleVideoSelection(video.id)}
					onReset={async (forceReset: boolean) => {
						await handleResetVideo(video.id, forceReset);
					}}
					onClearAndReset={async () => {
						await handleClearAndResetVideo(video.id);
					}}
					onRetry={async (videoId: number, taskIndex: number, isPage: boolean) => {
						try {
							if (isPage) {
								await api.retryPageTask(videoId, { task_index: taskIndex });
							} else {
								await api.retryVideoTask(videoId, { task_index: taskIndex });
							}
							// 重新加载视频列表
							const { query, currentPage, videoSource, statusFilter, sortBy, sortOrder } =
								$appStateStore;
							await loadVideos(query, currentPage, videoSource, statusFilter, sortBy, sortOrder);
							toast.success('已触发重试');
						} catch (error) {
							console.error('重试任务失败：', error);
							toast.error('重试任务失败', {
								description: (error as ApiError).message
							});
						}
					}}
				/>
			</div>
		{/each}
	</div>

	<!-- 翻页组件 -->
	<Pagination
		currentPage={$appStateStore.currentPage}
		{totalPages}
		onPageChange={handlePageChange}
	/>
{:else}
	<div class="flex items-center justify-center py-16">
		<div class="space-y-3 text-center">
			<p class="text-muted-foreground text-sm">暂无视频数据</p>
			<p class="text-muted-foreground/70 text-xs">尝试搜索或检查视频来源配置</p>
		</div>
	</div>
{/if}

<AlertDialog.Root bind:open={resetAllDialogOpen}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>{hasFilters ? '重置筛选视频' : '重置全部视频'}</AlertDialog.Title>
			<AlertDialog.Description>
				{#if hasFilters}
					确定要重置<strong>符合以下筛选条件</strong>的视频的下载状态吗？<br />
					<div class="bg-muted my-2 rounded-md p-2 text-left">
						{#each filterDescriptionParts as part, index (index)}
							<div><strong>{part}</strong></div>
						{/each}
					</div>
				{:else}
					确定要重置<strong>全部视频</strong>的下载状态吗？<br />
				{/if}
				此操作会将所有的失败状态重置为未开始，<span class="text-destructive font-medium"
					>无法撤销</span
				>。
			</AlertDialog.Description>
		</AlertDialog.Header>

		<div class="py-2">
			<div class="rounded-lg border border-orange-200 bg-orange-50 p-3">
				<div class="mb-2 flex items-center space-x-2">
					<Checkbox id="force-reset-all" bind:checked={forceReset} />
					<Label for="force-reset-all" class="text-sm font-medium text-orange-700"
						>⚠️ 强制重置</Label
					>
				</div>
				<p class="text-xs leading-relaxed text-orange-700">
					除重置失败状态外还会检查修复任务状态的标识位 <br />
					版本升级引入新任务时勾选该选项进行重置，可以允许旧视频执行新任务
				</p>
			</div>
		</div>

		<AlertDialog.Footer>
			<AlertDialog.Cancel
				disabled={resettingAll}
				onclick={() => {
					forceReset = false;
				}}>取消</AlertDialog.Cancel
			>
			<AlertDialog.Action
				onclick={handleResetAllVideos}
				disabled={resettingAll}
				class={forceReset ? 'bg-orange-600 hover:bg-orange-700' : ''}
			>
				{#if resettingAll}
					<RotateCcwIcon class="mr-2 h-4 w-4 animate-spin" />
					重置中...
				{:else}
					{forceReset ? '确认强制重置' : '确认重置'}
				{/if}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>

<FilteredStatusEditor
	bind:open={updateAllDialogOpen}
	{hasFilters}
	loading={updatingAll}
	filterDescriptionParts={filterDescriptionParts || []}
	onsubmit={handleUpdateAllVideos}
/>

<!-- 批量编辑对话框 -->
<BatchStatusEditor
	bind:open={batchEditDialogOpen}
	videoCount={selectedVideoIds.size}
	loading={batchEditing}
	onsubmit={handleBatchEditVideos}
/>
