// Velocty Admin — Minimal vanilla JS

(function() {
    'use strict';

    // Toast notifications
    function showToast(message, type) {
        const toast = document.createElement('div');
        toast.className = 'alert alert-' + (type || 'success');
        toast.textContent = message;
        toast.style.cssText = 'position:fixed;top:16px;right:16px;z-index:9999;min-width:250px;animation:fadeIn 200ms';
        document.body.appendChild(toast);
        setTimeout(() => toast.remove(), 3000);
    }

    // Sidebar hover expand — handled by CSS, but we add touch support
    const sidebar = document.getElementById('sidebar');
    if (sidebar) {
        sidebar.addEventListener('touchstart', function() {
            this.classList.toggle('expanded');
        });
    }

    // Confirm delete forms
    document.querySelectorAll('form[onsubmit]').forEach(form => {
        // Already handled inline
    });

    // Dashboard chart loading
    if (typeof d3 !== 'undefined' && document.getElementById('chart-sankey')) {
        loadDashboardCharts();
    }

    var chartColors = {
        accent: '#E8913A',
        accentHover: '#D07A2F',
        blue: '#446D7F',
        purple: '#8b5cf6',
        amber: '#f59e0b',
        red: '#ef4444',
        green: '#22c55e',
        pink: '#ec4899',
        cyan: '#06b6d4',
        textMuted: '#9ca3af',
        textDim: '#6b7280',
        bgDark: '#282B34',
        palette: ['#E8913A', '#446D7F', '#8b5cf6', '#f59e0b', '#22c55e', '#ef4444', '#ec4899', '#06b6d4']
    };

    async function loadDashboardCharts() {
        try {
            const [overview, flow, geo, stream, calendar, referrers, topPortfolio, tags] = await Promise.all([
                fetch('/admin/api/stats/overview').then(r => r.json()),
                fetch('/admin/api/stats/flow').then(r => r.json()),
                fetch('/admin/api/stats/geo').then(r => r.json()),
                fetch('/admin/api/stats/stream').then(r => r.json()),
                fetch('/admin/api/stats/calendar').then(r => r.json()),
                fetch('/admin/api/stats/top-referrers').then(r => r.json()),
                fetch('/admin/api/stats/top-portfolio').then(r => r.json()),
                fetch('/admin/api/stats/tags').then(r => r.json()),
            ]);

            if (flow && flow.length > 0) renderFlowChart('#chart-sankey', flow);
            if (geo && geo.length > 0) renderGeoChart('#chart-geo', geo);
            if (stream && stream.length > 0) renderStreamChart('#chart-stream', stream);
            if (calendar && calendar.length > 0) renderCalendarHeatmap('#chart-calendar', calendar);
            if (referrers && referrers.length > 0) renderHorizontalBars('#chart-referrers', referrers, chartColors.accent);
            if (topPortfolio && topPortfolio.length > 0) renderHorizontalBars('#chart-top-portfolio', topPortfolio, chartColors.blue);
            if (overview) renderSunburst('#chart-sunburst', overview, geo);

        } catch (e) {
            console.log('Dashboard charts: waiting for data', e);
        }
    }

    function renderFlowChart(selector, data) {
        var container = document.querySelector(selector);
        if (!container) return;
        container.innerHTML = '';

        // Group: referrer->content_type (first half) and content_type->page (second half)
        var refToType = data.filter(function(d) { return ['Blog','Portfolio','Pages'].indexOf(d.target) >= 0; });
        var typeToPage = data.filter(function(d) { return ['Blog','Portfolio','Pages'].indexOf(d.source) >= 0 && ['Blog','Portfolio','Pages'].indexOf(d.target) < 0; });

        var width = container.clientWidth || 600;
        var margin = {top: 10, right: 10, bottom: 10, left: 10};
        var colWidth = (width - margin.left - margin.right) / 3;

        // Collect unique nodes per column
        var sources = [];
        refToType.forEach(function(d) { if (sources.indexOf(d.source) < 0) sources.push(d.source); });
        var middles = ['Blog', 'Portfolio', 'Pages'];
        var targets = [];
        typeToPage.forEach(function(d) { if (targets.indexOf(d.target) < 0) targets.push(d.target); });
        targets = targets.slice(0, 8);

        var nodeH = 22, nodeGap = 4;
        var height = Math.max(sources.length, middles.length, targets.length) * (nodeH + nodeGap) + 40;

        var svg = d3.select(selector).append('svg')
            .attr('width', width).attr('height', height);

        var typeColor = {Blog: chartColors.accent, Portfolio: chartColors.blue, Pages: chartColors.purple};

        // Draw source nodes (referrers)
        var srcY = function(i) { return margin.top + i * (nodeH + nodeGap); };
        sources.forEach(function(s, i) {
            var label = s.replace('https://', '').replace('http://', '').replace(/\/$/, '') || 'Direct';
            svg.append('text').attr('x', margin.left).attr('y', srcY(i) + nodeH/2 + 4)
                .attr('fill', chartColors.textMuted).attr('font-size', '11px').text(label);
        });

        // Draw middle nodes (content types)
        var midX = margin.left + colWidth;
        var midY = function(i) { return margin.top + i * (nodeH + nodeGap + 8); };
        var midTotals = {};
        refToType.forEach(function(d) { midTotals[d.target] = (midTotals[d.target] || 0) + d.value; });
        middles.forEach(function(m, i) {
            var total = midTotals[m] || 0;
            if (total === 0) return;
            var barW = Math.min(total * 2, colWidth - 40);
            svg.append('rect').attr('x', midX).attr('y', midY(i)).attr('width', 0).attr('height', nodeH)
                .attr('fill', typeColor[m] || chartColors.accent).attr('rx', 3).attr('opacity', 0.8)
                .transition().duration(600).attr('width', barW);
            svg.append('text').attr('x', midX + barW + 6).attr('y', midY(i) + nodeH/2 + 4)
                .attr('fill', chartColors.textMuted).attr('font-size', '11px').text(m + ' (' + total + ')');
        });

        // Draw links from sources to middles
        refToType.forEach(function(d) {
            var si = sources.indexOf(d.source);
            var mi = middles.indexOf(d.target);
            if (si < 0 || mi < 0 || !midTotals[d.target]) return;
            svg.append('line')
                .attr('x1', margin.left + 80).attr('y1', srcY(si) + nodeH/2)
                .attr('x2', midX).attr('y2', midY(mi) + nodeH/2)
                .attr('stroke', typeColor[d.target] || chartColors.accent).attr('stroke-opacity', 0.25)
                .attr('stroke-width', Math.max(1, Math.min(d.value / 2, 6)));
        });

        // Draw target nodes (top pages)
        var tgtX = margin.left + colWidth * 2;
        var tgtY = function(i) { return margin.top + i * (nodeH + nodeGap); };
        typeToPage.slice(0, 8).forEach(function(d, i) {
            var label = d.target.replace(/^\/portfolio\//, '').replace(/^\/journal\//, '').replace(/^\/blog\//, '');
            if (label.length > 20) label = label.substring(0, 20) + '…';
            svg.append('text').attr('x', tgtX + 6).attr('y', tgtY(i) + nodeH/2 + 4)
                .attr('fill', chartColors.textMuted).attr('font-size', '11px').text(label + ' (' + d.value + ')');

            // Link from middle to target
            var mi = middles.indexOf(d.source);
            if (mi >= 0) {
                svg.append('line')
                    .attr('x1', midX + 60).attr('y1', midY(mi) + nodeH/2)
                    .attr('x2', tgtX).attr('y2', tgtY(i) + nodeH/2)
                    .attr('stroke', typeColor[d.source] || chartColors.accent).attr('stroke-opacity', 0.2)
                    .attr('stroke-width', Math.max(1, Math.min(d.value / 2, 4)));
            }
        });
    }

    function renderSunburst(selector, overview, geo) {
        var container = document.querySelector(selector);
        if (!container) return;
        container.innerHTML = '';

        var width = container.clientWidth || 300;
        var height = 260;
        var radius = Math.min(width, height) / 2 - 20;

        var data = [
            {label: 'Portfolio', value: overview.portfolio_count || 0},
            {label: 'Journal', value: overview.posts_count || 0},
            {label: 'Comments', value: overview.comments_pending || 0},
        ];
        var total = overview.total_views || 0;

        var svg = d3.select(selector).append('svg')
            .attr('width', width).attr('height', height);
        var g = svg.append('g').attr('transform', 'translate(' + width/2 + ',' + height/2 + ')');

        var pie = d3.pie().value(function(d) { return d.value; }).sort(null);
        var arc = d3.arc().innerRadius(radius * 0.55).outerRadius(radius);

        g.selectAll('path').data(pie(data)).join('path')
            .attr('d', arc)
            .attr('fill', function(d, i) { return chartColors.palette[i]; })
            .attr('stroke', chartColors.bgDark).attr('stroke-width', 2)
            .append('title').text(function(d) { return d.data.label + ': ' + d.data.value; });

        // Center text
        g.append('text').attr('text-anchor', 'middle').attr('dy', '-0.2em')
            .attr('fill', '#f0f0f0').attr('font-size', '22px').attr('font-weight', '600')
            .text(total);
        g.append('text').attr('text-anchor', 'middle').attr('dy', '1.2em')
            .attr('fill', chartColors.textMuted).attr('font-size', '11px')
            .text('total views');

        // Legend
        data.forEach(function(d, i) {
            g.append('circle').attr('cx', -radius + 5).attr('cy', radius + 12 + i * 0)
                .attr('r', 0); // skip legend if tight
        });
    }

    function renderGeoChart(selector, data) {
        var container = document.querySelector(selector);
        if (!container || !data.length) return;
        container.innerHTML = '';

        var top = data.slice(0, 12);
        var width = container.clientWidth || 300;
        var barHeight = 24;
        var height = top.length * barHeight + 10;
        var maxCount = Math.max.apply(null, top.map(function(d) { return d.count; }));
        var total = data.reduce(function(s, d) { return s + d.count; }, 0);

        var svg = d3.select(selector).append('svg')
            .attr('width', width).attr('height', height);

        var labelW = 40, barW = width - labelW - 60;

        svg.selectAll('g').data(top).join('g')
            .attr('transform', function(d, i) { return 'translate(0,' + (i * barHeight) + ')'; })
            .each(function(d) {
                var g = d3.select(this);
                g.append('text').attr('x', 0).attr('y', barHeight/2 + 4)
                    .attr('fill', chartColors.textMuted).attr('font-size', '11px').attr('font-weight', '500')
                    .text(d.label);
                g.append('rect').attr('x', labelW).attr('y', 4).attr('width', 0)
                    .attr('height', barHeight - 8).attr('fill', chartColors.blue).attr('rx', 3).attr('opacity', 0.8)
                    .transition().duration(600).attr('width', (d.count / maxCount) * barW);
                var pct = total > 0 ? Math.round(d.count / total * 100) : 0;
                g.append('text').attr('x', labelW + (d.count / maxCount) * barW + 6).attr('y', barHeight/2 + 4)
                    .attr('fill', chartColors.textDim).attr('font-size', '10px')
                    .text(d.count + ' (' + pct + '%)');
            });
    }

    function renderStreamChart(selector, data) {
        var container = document.querySelector(selector);
        if (!container || !data.length) return;
        container.innerHTML = '';

        // Group by date
        var dates = [];
        var byDate = {};
        data.forEach(function(d) {
            if (!byDate[d.date]) { byDate[d.date] = {}; dates.push(d.date); }
            byDate[d.date][d.content_type] = d.count;
        });
        dates.sort();

        var types = ['portfolio', 'blog', 'pages'];
        var typeLabels = {portfolio: 'Portfolio', blog: 'Journal', pages: 'Pages'};
        var typeColors = {portfolio: chartColors.accent, blog: chartColors.blue, pages: chartColors.purple};

        var width = container.clientWidth || 600;
        var height = 180;
        var margin = {top: 20, right: 10, bottom: 30, left: 30};
        var innerW = width - margin.left - margin.right;
        var innerH = height - margin.top - margin.bottom;

        var maxTotal = 0;
        dates.forEach(function(d) {
            var t = 0;
            types.forEach(function(tp) { t += (byDate[d][tp] || 0); });
            if (t > maxTotal) maxTotal = t;
        });

        var x = d3.scaleBand().domain(dates).range([0, innerW]).padding(0.15);
        var y = d3.scaleLinear().domain([0, maxTotal]).range([innerH, 0]);

        var svg = d3.select(selector).append('svg')
            .attr('width', width).attr('height', height);
        var g = svg.append('g').attr('transform', 'translate(' + margin.left + ',' + margin.top + ')');

        // Stacked bars
        dates.forEach(function(date) {
            var cumY = 0;
            types.forEach(function(tp) {
                var val = byDate[date][tp] || 0;
                if (val === 0) return;
                g.append('rect')
                    .attr('x', x(date)).attr('y', y(cumY + val))
                    .attr('width', x.bandwidth()).attr('height', y(cumY) - y(cumY + val))
                    .attr('fill', typeColors[tp]).attr('rx', 2).attr('opacity', 0.85)
                    .append('title').text(typeLabels[tp] + ': ' + val + ' on ' + date);
                cumY += val;
            });
        });

        // X axis (show every Nth label)
        var step = Math.max(1, Math.floor(dates.length / 8));
        var tickDates = dates.filter(function(d, i) { return i % step === 0; });
        g.append('g').attr('transform', 'translate(0,' + innerH + ')')
            .call(d3.axisBottom(x).tickValues(tickDates).tickFormat(function(d) {
                return d.substring(5); // MM-DD
            }))
            .selectAll('text').attr('fill', chartColors.textDim).attr('font-size', '9px');
        g.selectAll('.domain, .tick line').attr('stroke', '#363840');

        // Y axis
        g.append('g').call(d3.axisLeft(y).ticks(4))
            .selectAll('text').attr('fill', chartColors.textDim).attr('font-size', '9px');
        g.selectAll('.domain, .tick line').attr('stroke', '#363840');

        // Legend
        var legendX = innerW - 180;
        types.forEach(function(tp, i) {
            svg.append('rect').attr('x', margin.left + legendX + i * 70).attr('y', 4)
                .attr('width', 10).attr('height', 10).attr('rx', 2).attr('fill', typeColors[tp]);
            svg.append('text').attr('x', margin.left + legendX + i * 70 + 14).attr('y', 13)
                .attr('fill', chartColors.textMuted).attr('font-size', '10px').text(typeLabels[tp]);
        });
    }

    function renderHorizontalBars(selector, data, color) {
        var container = document.querySelector(selector);
        if (!container || !data.length) return;
        container.innerHTML = '';

        var top = data.slice(0, 10);
        var width = container.clientWidth || 300;
        var barHeight = 28;
        var height = top.length * barHeight + 10;
        var maxCount = Math.max.apply(null, top.map(function(d) { return d.count; }));

        var svg = d3.select(selector).append('svg')
            .attr('width', width).attr('height', height);

        var labelW = 130, barW = width - labelW - 50;

        svg.selectAll('g').data(top).join('g')
            .attr('transform', function(d, i) { return 'translate(0,' + (i * barHeight) + ')'; })
            .each(function(d) {
                var g = d3.select(this);
                var label = d.label.replace(/^\/portfolio\//, '').replace(/^\/journal\//, '').replace(/^\/blog\//, '')
                    .replace('https://', '').replace('http://', '').replace(/\/$/, '');
                if (!label) label = 'Direct';
                if (label.length > 18) label = label.substring(0, 18) + '…';

                g.append('text').attr('x', 0).attr('y', barHeight/2 + 4)
                    .attr('fill', chartColors.textMuted).attr('font-size', '12px').text(label);

                g.append('rect').attr('x', labelW).attr('y', 4).attr('width', 0)
                    .attr('height', barHeight - 8).attr('fill', color || chartColors.accent).attr('rx', 3)
                    .transition().duration(600).attr('width', (d.count / maxCount) * barW);

                g.append('text').attr('x', labelW + (d.count / maxCount) * barW + 8).attr('y', barHeight/2 + 4)
                    .attr('fill', chartColors.textDim).attr('font-size', '11px').text(d.count);
            });
    }

    function renderCalendarHeatmap(selector, data) {
        var container = document.querySelector(selector);
        if (!container || !data.length) return;
        container.innerHTML = '';

        var cellSize = 14, gap = 2;
        var width = container.clientWidth || 700;
        var height = cellSize * 7 + 50;
        var maxCount = Math.max.apply(null, data.map(function(d) { return d.count; }));

        var colorScale = d3.scaleLinear().domain([0, maxCount]).range([chartColors.bgDark, chartColors.accent]);

        var svg = d3.select(selector).append('svg')
            .attr('width', width).attr('height', height);

        // Sort by date and compute positions
        data.sort(function(a, b) { return a.date.localeCompare(b.date); });
        var firstDate = new Date(data[0].date + 'T00:00:00');
        var firstDay = firstDate.getDay(); // 0=Sun

        svg.selectAll('rect').data(data).join('rect')
            .attr('x', function(d) {
                var dt = new Date(d.date + 'T00:00:00');
                var daysDiff = Math.floor((dt - firstDate) / 86400000);
                var week = Math.floor((daysDiff + firstDay) / 7);
                return week * (cellSize + gap) + 30;
            })
            .attr('y', function(d) {
                var dt = new Date(d.date + 'T00:00:00');
                var dow = dt.getDay();
                return dow * (cellSize + gap) + 15;
            })
            .attr('width', cellSize).attr('height', cellSize).attr('rx', 2)
            .attr('fill', function(d) { return colorScale(d.count); })
            .append('title').text(function(d) { return d.date + ': ' + d.count + ' views'; });

        // Day labels
        var days = ['Sun','Mon','Tue','Wed','Thu','Fri','Sat'];
        days.forEach(function(d, i) {
            if (i % 2 === 1) {
                svg.append('text').attr('x', 0).attr('y', i * (cellSize + gap) + 15 + cellSize/2 + 3)
                    .attr('fill', chartColors.textDim).attr('font-size', '9px').text(d);
            }
        });
    }

})();
