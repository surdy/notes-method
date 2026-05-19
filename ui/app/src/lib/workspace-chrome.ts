export type WorkspaceChromeState = {
	leftSidebarCollapsed: boolean;
	rightRailCollapsed: boolean;
};

export type WorkspaceChromeLayout = {
	leftChromeWidth: string;
	rightChromeWidth: string;
	leftToggleLabel: string;
	rightToggleLabel: string;
	leftToggleIcon: 'panel-left';
	rightToggleIcon: 'panel-right';
};

const LEFT_SIDEBAR_WIDTH = '280px';
const RIGHT_RAIL_WIDTH = '260px';
const COLLAPSED_SIDE_WIDTH = '44px';

export function workspaceChromeLayout({
	leftSidebarCollapsed,
	rightRailCollapsed
}: WorkspaceChromeState): WorkspaceChromeLayout {
	return {
		leftChromeWidth: leftSidebarCollapsed ? COLLAPSED_SIDE_WIDTH : LEFT_SIDEBAR_WIDTH,
		rightChromeWidth: rightRailCollapsed ? COLLAPSED_SIDE_WIDTH : RIGHT_RAIL_WIDTH,
		leftToggleLabel: leftSidebarCollapsed ? 'Expand left sidebar' : 'Collapse left sidebar',
		rightToggleLabel: rightRailCollapsed ? 'Expand right sidebar' : 'Collapse right sidebar',
		leftToggleIcon: 'panel-left',
		rightToggleIcon: 'panel-right'
	};
}
