# Velocty — Manual Testing Scenarios

This document covers visual and interactive test scenarios that cannot be fully verified by unit tests. Each scenario lists the **precondition** (settings to configure), **steps**, and **expected result**.

> **How to change settings:** Go to `/admin/designs` → click the active design → Customizer.
> After saving, refresh the public site to see changes.

---

## 1. Layout Switching

### 1.1 Sidebar Layout (default)
- **Precondition:** Layout → Header Type = **Sidebar**
- **Steps:** Open the public site
- **Expected:**
  - Fixed sidebar on the left with logo, site name, tagline, nav links, social icons
  - Content area fills the remaining width
  - Mobile: sidebar hidden, hamburger menu in mobile header bar

### 1.2 Top Bar Layout
- **Precondition:** Layout → Header Type = **Top Bar**
- **Steps:** Open the public site
- **Expected:**
  - Horizontal header bar at the top with logo, site name, nav links
  - No sidebar visible
  - Content area spans full width below the header

### 1.3 Sidebar Position (Left / Right)
- **Precondition:** Header Type = Sidebar, Sidebar Position = **Right**
- **Steps:** Open the public site
- **Expected:**
  - Sidebar appears on the right side
  - Content area on the left
- **Reset:** Change back to Left

### 1.4 Boxed Mode
- **Precondition:** Content Boundary = **Boxed**
- **Steps:** Open the public site on a wide monitor (>1400px)
- **Expected:**
  - Content is constrained to a max-width container
  - Centered on the page with background visible on sides
- **Test with both:** Sidebar and Top Bar layouts

### 1.5 Sidebar Custom Heading & Text
- **Precondition:** Header Type = Sidebar, set Sidebar Custom Heading = "About Me", Sidebar Custom Text = "A short bio"
- **Steps:** Open the public site
- **Expected:**
  - "About Me" heading and "A short bio" text visible in the sidebar below nav
- **Then:** Switch Header Type to Top Bar
- **Expected:**
  - Custom heading and text are NOT visible (sidebar-only feature)

---

## 2. Navigation

### 2.1 Navigation Position — Below Logo (Top Bar)
- **Precondition:** Header Type = Top Bar, Navigation Position = **Below Logo**
- **Steps:** Open the public site
- **Expected:**
  - Logo/site name on the first row (full width)
  - Nav links on a second row below the logo

### 2.2 Navigation Position — Right (Top Bar)
- **Precondition:** Header Type = Top Bar, Navigation Position = **Right**
- **Steps:** Open the public site
- **Expected:**
  - Logo on the left, nav links pushed to the right on the same row
  - Social/share icons (if enabled) appear after the nav links

### 2.3 Responsive Hamburger Menu (Top Bar)
- **Precondition:** Header Type = Top Bar
- **Steps:** Resize browser to ≤900px width (or use mobile device)
- **Expected:**
  - Nav links collapse and are hidden
  - Hamburger button (☰) appears on the right
  - Clicking hamburger toggles nav links vertically
  - Hamburger icon animates to an X when open
  - Social/share icons appear below nav when menu is open

### 2.4 Responsive Sidebar (Sidebar Layout)
- **Precondition:** Header Type = Sidebar
- **Steps:** Resize browser to ≤900px width
- **Expected:**
  - Sidebar hidden, mobile header bar appears with logo + hamburger
  - Clicking hamburger slides sidebar in/out

---

## 3. Portfolio Category Filters

### 3.1 Sidebar + Under Portfolio Link
- **Precondition:** Header = Sidebar, Portfolio Category Filters = **Under Portfolio Link**
- **Steps:** Open portfolio page
- **Expected:**
  - Portfolio label appears as a collapsible toggle (with ▾ arrow)
  - Clicking toggle shows/hides category links (All, [categories])
  - Toggle starts **open** by default
  - Portfolio does NOT appear as a separate nav link (toggle replaces it)
  - Clicking "All" shows all items; clicking a category filters

### 3.2 Sidebar + Page Top
- **Precondition:** Header = Sidebar, Portfolio Category Filters = **Page Top**, Alignment = **Left**
- **Steps:** Open portfolio page
- **Expected:**
  - Portfolio appears as a normal nav link in the sidebar
  - Horizontal category links (All, [categories]) appear at the top of the content area
  - Links are left-aligned
- **Then:** Change Alignment to **Right**
- **Expected:**
  - Category links are right-aligned

### 3.3 Top Bar + Sub-menu
- **Precondition:** Header = Top Bar, Portfolio Category Filters = **Sub-menu**
- **Steps:** Open portfolio page
- **Expected:**
  - Portfolio label appears in the nav bar as a dropdown toggle (with ▾ arrow)
  - Dropdown starts **closed**
  - Clicking toggle opens a floating dropdown with All + category links
  - Portfolio does NOT appear as a separate nav link
  - Active category is highlighted (bold + accent color)
- **Mobile (≤900px):** Dropdown becomes inline (no floating box)

### 3.4 Top Bar + Below Main Menu
- **Precondition:** Header = Top Bar, Portfolio Category Filters = **Below Main Menu**
- **Steps:** Open portfolio page
- **Expected:**
  - Portfolio appears as a normal nav link in the top bar
  - A second horizontal row of category links appears below the header
  - Row has a subtle bottom border
  - Active category is highlighted

### 3.5 Hidden Categories
- **Precondition:** Portfolio Category Filters = **Hidden**
- **Steps:** Open portfolio page
- **Expected:**
  - No category links anywhere
  - Portfolio appears as a normal nav link
  - All portfolio items shown (no filtering)

### 3.6 Customizer Option Visibility
- **Steps:** Open the customizer
- **When Header = Sidebar:**
  - Category filter options should be: Hidden, Under Portfolio Link, Page Top
  - Sub-menu and Below Main Menu should NOT be visible
  - Page Top Alignment field visible only when Page Top is selected
- **When Header = Top Bar:**
  - Category filter options should be: Hidden, Sub-menu, Below Main Menu
  - Under Portfolio Link and Page Top should NOT be visible

### 3.7 Show Categories — Don't Show (default)
- **Precondition:** Show Categories = **Don't Show**
- **Steps:** Open portfolio page with items that have categories assigned
- **Expected:**
  - No visible category labels on grid items
  - `data-categories` attribute still present on each grid item (for JS filtering)

### 3.8 Show Categories — On Hover
- **Precondition:** Show Categories = **On Hover**
- **Steps:** Open portfolio page, hover over a grid item
- **Expected:**
  - Category names appear as a dark semi-transparent bar at the bottom of the image on hover
  - Bar fades in smoothly (opacity transition)
  - Bar disappears when mouse leaves the item
  - Category names are clickable links

### 3.9 Show Categories — Bottom Left (over image)
- **Precondition:** Show Categories = **Bottom Left (over image)**
- **Steps:** Open portfolio page
- **Expected:**
  - Category names appear as a dark pill/badge at the bottom-left corner of the image
  - Always visible (not hover-dependent)
  - Rounded corner on top-right

### 3.10 Show Categories — Bottom Right (over image)
- **Precondition:** Show Categories = **Bottom Right (over image)**
- **Expected:**
  - Same as Bottom Left but positioned at bottom-right corner
  - Rounded corner on top-left

### 3.11 Show Categories — Below Left
- **Precondition:** Show Categories = **Below Left**
- **Expected:**
  - Category names appear below the image, left-aligned
  - Not overlapping the image
  - Uses the Categories font and color from Typography/Colors settings

### 3.12 Show Categories — Below Right
- **Precondition:** Show Categories = **Below Right**
- **Expected:**
  - Category names appear below the image, right-aligned

### 3.13 Show Tags — All Position Modes
- **Precondition:** Test each Show Tags option (same 6 modes as categories)
- **Expected:** Same positioning behavior as categories but for tag labels
  - Tags use the Tags font and color from Typography/Colors settings
  - Tags are separated by " · " (middle dot)

### 3.14 Mixed Modes — Categories Hover + Tags Below
- **Precondition:** Show Categories = **On Hover**, Show Tags = **Below Left**
- **Steps:** Open portfolio page, hover over a grid item
- **Expected:**
  - Tags always visible below the image (left-aligned)
  - Categories appear on hover as overlay on the image
  - Both render independently — hover works even when tags use a different mode

### 3.15 Legacy "true" Migration
- **Precondition:** Existing database with old `portfolio_show_categories = "true"` value
- **Steps:** Restart the server
- **Expected:**
  - Value auto-migrated to `"false"` (Don't Show)
  - Customizer shows "Don't Show" selected

---

## 4. Social Icons

### 4.1 Social Icons in Sidebar
- **Precondition:** Social Icons Position = **Sidebar**, set at least one social URL (e.g. Instagram)
- **Steps:** Open the public site
- **Expected:**
  - Social icons visible in the sidebar bottom area
  - Clicking opens the social URL in a new tab
  - Hover shows the platform name as tooltip

### 4.2 Social Icons in Footer
- **Precondition:** Social Icons Position = **Footer**
- **Steps:** Scroll to the footer
- **Expected:**
  - Social icons visible in the footer area
  - Not visible in the sidebar

### 4.3 Social Icons in Both
- **Precondition:** Social Icons Position = **Both**
- **Expected:** Icons visible in both sidebar and footer

### 4.4 Brand Colors
- **Precondition:** Enable Brand Colors toggle, set multiple social URLs
- **Expected:**
  - Each icon uses its brand color (Instagram = pink, Facebook = blue, etc.)
- **Disable Brand Colors:**
  - All icons use the default text color

### 4.5 No Social URLs
- **Precondition:** All social URL fields empty
- **Expected:** No social icons section rendered at all (no empty container)

### 4.6 Social Icons in Top Bar
- **Precondition:** Header = Top Bar, Social Icons Position = Sidebar (or Both)
- **Expected:** Social icons appear in the top bar right section

---

## 5. Share Icons

### 5.1 Share Icons in Sidebar
- **Precondition:** Share Enabled = true, Share Facebook = true, Share Icons Position = **Sidebar**, Site URL set
- **Steps:** Open the public site
- **Expected:**
  - Share icons visible in the sidebar
  - Clicking opens the share URL (e.g. Facebook sharer) in a new tab

### 5.2 Share Label
- **Precondition:** Share Label = "Share:"
- **Expected:**
  - "Share:" text appears before the share icons
  - Styled inline with the icons

### 5.3 Share Disabled
- **Precondition:** Share Enabled = false
- **Expected:** No share icons rendered anywhere

### 5.4 Share Below Content
- **Precondition:** Share Icons Position = **Below Content**
- **Steps:** Open a portfolio single page or blog post
- **Expected:**
  - Share icons appear below the content, not in the sidebar

---

## 6. Footer & Copyright

### 6.1 Copyright Center Alignment
- **Precondition:** Copyright Text = "© 2026 My Site", Copyright Alignment = **Center**
- **Steps:** Scroll to footer
- **Expected:** Copyright text centered in the footer

### 6.2 Copyright Left Alignment
- **Precondition:** Copyright Alignment = **Left**
- **Expected:** Copyright text left-aligned

### 6.3 Copyright Right Alignment
- **Precondition:** Copyright Alignment = **Right**
- **Expected:** Copyright text right-aligned

### 6.4 Footer 3-Column Grid
- **Precondition:**
  - Copyright Text set, Copyright Alignment = **Left**
  - Social Icons Position = **Footer**, Footer Alignment = **Right**
  - At least one social URL set
- **Expected:**
  - Footer has 3 columns: copyright on the left, social icons on the right
  - Center column empty but maintains spacing

### 6.5 Empty Footer
- **Precondition:** No copyright text, Social Icons Position = Sidebar (not footer)
- **Expected:** Footer area is empty / minimal height

---

## 7. Site Identity

### 7.1 Site Name Display — Text
- **Precondition:** Site Name Display = **Text**
- **Expected:** Site name shown as text, no logo image

### 7.2 Site Name Display — Logo
- **Precondition:** Site Name Display = **Logo**, upload a logo image
- **Expected:** Logo image shown, site name text hidden

### 7.3 Site Name Display — Both
- **Precondition:** Site Name Display = **Both**
- **Expected:** Both logo image and site name text visible

### 7.4 Tagline
- **Precondition:** Tagline Enabled = true, set a tagline text
- **Expected:** Tagline visible below site name
- **Disable tagline:** Tagline text hidden

---

## 8. Cross-Feature Interactions

### 8.1 Top Bar + Boxed + Below Menu Categories
- **Precondition:** Header = Top Bar, Boxed Mode, Categories = Below Main Menu
- **Expected:**
  - Header constrained to max-width
  - Category row below header also constrained
  - Content area constrained

### 8.2 Top Bar + Right Nav + Sub-menu Categories
- **Precondition:** Header = Top Bar, Nav Position = Right, Categories = Sub-menu
- **Expected:**
  - Nav links right-aligned with portfolio dropdown toggle
  - Dropdown opens below the toggle, not clipped

### 8.3 Sidebar + Page Top Categories + Share in Sidebar
- **Precondition:** Header = Sidebar, Categories = Page Top, Share Position = Sidebar
- **Expected:**
  - Share icons in sidebar
  - Category links at top of portfolio content
  - Both visible simultaneously

### 8.4 Switch Header Type Preserves Settings
- **Steps:**
  1. Set Header = Sidebar, configure all sidebar settings
  2. Switch to Top Bar, save
  3. Switch back to Sidebar
- **Expected:** All sidebar settings (position, custom text, etc.) preserved

---

## 9. Customizer Conditional Fields

### 9.1 Header Type Toggle
- **Steps:** Toggle Header Type between Sidebar and Top Bar
- **Expected fields visibility:**

| Field | Sidebar | Top Bar |
|---|---|---|
| Sidebar Position | ✅ | ❌ |
| Sidebar Custom Heading | ✅ | ❌ |
| Sidebar Custom Text | ✅ | ❌ |
| Navigation Position | ❌ | ✅ |

### 9.2 Category Filter Options Toggle
- **Steps:** Toggle Header Type and observe Category Filter dropdown
- **Expected:**

| Option | Sidebar | Top Bar |
|---|---|---|
| Hidden | ✅ | ✅ |
| Under Portfolio Link | ✅ | ❌ |
| Page Top | ✅ | ❌ |
| Sub-menu | ❌ | ✅ |
| Below Main Menu | ❌ | ✅ |

### 9.3 Page Top Alignment
- **Steps:** Set Header = Sidebar, Category Filters = Page Top
- **Expected:** Page Top Alignment dropdown (Left/Right) becomes visible
- **Change to Under Portfolio Link:** Alignment dropdown hides

### 9.4 Auto-Reset on Header Switch
- **Steps:**
  1. Set Header = Sidebar, Category Filters = Under Portfolio Link
  2. Switch Header to Top Bar
- **Expected:** Category Filters auto-resets to first visible option (Hidden) since Under Portfolio Link is sidebar-only
