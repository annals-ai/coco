#!/usr/bin/env python3
"""Generate PDF for 2028 Global Intelligence Crisis - Original English version."""

import os
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
from reportlab.lib.units import mm, cm
from reportlab.lib.colors import HexColor
from reportlab.lib.enums import TA_LEFT, TA_CENTER, TA_JUSTIFY
from reportlab.platypus import (
    SimpleDocTemplate, Paragraph, Spacer, Image, PageBreak,
    HRFlowable, Table, TableStyle,
)
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from PIL import Image as PILImage

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
IMG_DIR = os.path.join(BASE_DIR, 'images')

PAGE_W, PAGE_H = A4
MARGIN_L = 25 * mm
MARGIN_R = 25 * mm
MARGIN_T = 20 * mm
MARGIN_B = 20 * mm
CONTENT_W = PAGE_W - MARGIN_L - MARGIN_R

C_BLACK = HexColor('#1a1a1a')
C_DARK = HexColor('#333333')
C_GRAY = HexColor('#666666')
C_LIGHT = HexColor('#999999')
C_ACCENT = HexColor('#2c5282')
C_RULE = HexColor('#d0d0d0')

# Use built-in fonts for English
FONT_SERIF = 'Times-Roman'
FONT_SERIF_B = 'Times-Bold'
FONT_SERIF_I = 'Times-Italic'
FONT_SANS = 'Helvetica'
FONT_SANS_B = 'Helvetica-Bold'

s_title = ParagraphStyle(
    'Title', fontName=FONT_SERIF_B, fontSize=28, leading=36,
    textColor=C_BLACK, alignment=TA_CENTER, spaceAfter=6*mm, spaceBefore=15*mm
)
s_subtitle = ParagraphStyle(
    'Subtitle', fontName=FONT_SERIF_I, fontSize=12, leading=17,
    textColor=C_GRAY, alignment=TA_CENTER, spaceAfter=3*mm
)
s_h1 = ParagraphStyle(
    'H1', fontName=FONT_SERIF_B, fontSize=20, leading=28,
    textColor=C_BLACK, spaceBefore=12*mm, spaceAfter=5*mm
)
s_h2 = ParagraphStyle(
    'H2', fontName=FONT_SERIF_B, fontSize=16, leading=24,
    textColor=C_ACCENT, spaceBefore=8*mm, spaceAfter=4*mm
)
s_body = ParagraphStyle(
    'Body', fontName=FONT_SERIF, fontSize=10.5, leading=18,
    textColor=C_DARK, alignment=TA_JUSTIFY, spaceAfter=3*mm
)
s_quote = ParagraphStyle(
    'Quote', fontName=FONT_SANS, fontSize=9.5, leading=15,
    textColor=C_DARK, alignment=TA_LEFT, spaceAfter=4*mm, spaceBefore=3*mm,
    leftIndent=8*mm, rightIndent=5*mm,
    backColor=HexColor('#f7f7f7'),
)
s_caption = ParagraphStyle(
    'Caption', fontName=FONT_SANS, fontSize=8.5, leading=13,
    textColor=C_LIGHT, alignment=TA_CENTER, spaceAfter=5*mm, spaceBefore=2*mm
)


def hr():
    return HRFlowable(width="100%", thickness=0.5, color=C_RULE,
                      spaceBefore=4*mm, spaceAfter=4*mm)

def thin_hr():
    return HRFlowable(width="40%", thickness=0.3, color=C_RULE,
                      spaceBefore=3*mm, spaceAfter=3*mm, hAlign='CENTER')

def sp(h=3):
    return Spacer(1, h*mm)

def img(filename, caption=None, max_w=None):
    path = os.path.join(IMG_DIR, filename)
    if not os.path.exists(path):
        return [Paragraph(f"[Image missing: {filename}]", s_caption)]
    pil = PILImage.open(path)
    iw, ih = pil.size
    target_w = max_w or CONTENT_W
    ratio = target_w / iw
    target_h = ih * ratio
    max_h = (PAGE_H - MARGIN_T - MARGIN_B) * 0.55
    if target_h > max_h:
        ratio = max_h / ih
        target_w = iw * ratio
        target_h = ih * ratio
    elements = [sp(2), Image(path, width=target_w, height=target_h, hAlign='CENTER')]
    if caption:
        elements.append(Paragraph(caption, s_caption))
    else:
        elements.append(sp(3))
    return elements

def quote_block(text):
    inner = Paragraph(text, s_quote)
    t = Table(
        [[' ', inner]],
        colWidths=[2.5*mm, CONTENT_W - 8*mm],
        style=TableStyle([
            ('BACKGROUND', (0, 0), (0, 0), C_ACCENT),
            ('BACKGROUND', (1, 0), (1, 0), HexColor('#f7f7f7')),
            ('TOPPADDING', (1, 0), (1, 0), 3*mm),
            ('BOTTOMPADDING', (1, 0), (1, 0), 3*mm),
            ('LEFTPADDING', (1, 0), (1, 0), 4*mm),
            ('RIGHTPADDING', (1, 0), (1, 0), 4*mm),
            ('LEFTPADDING', (0, 0), (0, 0), 0),
            ('RIGHTPADDING', (0, 0), (0, 0), 0),
            ('TOPPADDING', (0, 0), (0, 0), 0),
            ('BOTTOMPADDING', (0, 0), (0, 0), 0),
            ('VALIGN', (0, 0), (-1, -1), 'TOP'),
        ])
    )
    return [sp(2), t, sp(2)]

def p(text):
    return Paragraph(text, s_body)

def h1(text):
    return Paragraph(text, s_h1)

def h2(text):
    return Paragraph(text, s_h2)

def add_page_number(canvas_obj, doc):
    canvas_obj.saveState()
    canvas_obj.setFont(FONT_SANS, 8)
    canvas_obj.setFillColor(C_LIGHT)
    canvas_obj.drawCentredString(PAGE_W / 2, 12 * mm, f"- {doc.page} -")
    canvas_obj.restoreState()


def build():
    output_path = os.path.join(BASE_DIR, 'The 2028 Global Intelligence Crisis.pdf')
    doc = SimpleDocTemplate(
        output_path, pagesize=A4,
        leftMargin=MARGIN_L, rightMargin=MARGIN_R,
        topMargin=MARGIN_T, bottomMargin=MARGIN_B,
        title='The 2028 Global Intelligence Crisis',
        author='Citrini Research & Alap Shah',
    )

    story = []

    # === COVER ===
    story.append(sp(20))
    story.extend(img('01_cover.jpeg', max_w=CONTENT_W * 0.55))
    story.append(sp(5))
    story.append(Paragraph('The 2028 Global<br/>Intelligence Crisis', s_title))
    story.append(Paragraph('A Thought Exercise in Financial History, from the Future', s_subtitle))
    story.append(sp(3))
    story.append(Paragraph('Citrini Research &amp; Alap Shah', s_subtitle))
    story.append(Paragraph('February 22, 2026', s_subtitle))
    story.append(sp(5))
    story.append(thin_hr())
    story.append(Paragraph('citriniresearch.com/p/2028gic', s_caption))
    story.append(PageBreak())

    # === PREFACE ===
    story.append(h1('Preface'))
    story.append(p(
        'What if our AI bullishness continues to be right...and what if that\'s actually bearish?'
    ))
    story.append(p(
        'What follows is a scenario, not a prediction. This isn\'t bear porn or AI doomer fan-fiction. '
        'The sole intent of this piece is modeling a scenario that\'s been relatively underexplored. '
        'Our friend Alap Shah posed the question, and together we brainstormed the answer. We wrote this '
        'part, and he\'s written two others you can find here.'
    ))
    story.append(p(
        'Hopefully, reading this leaves you more prepared for potential left tail risks as AI makes the '
        'economy increasingly weird.'
    ))
    story.append(p(
        'This is the CitriniResearch Macro Memo from June 2028, detailing the progression and fallout '
        'of the Global Intelligence Crisis.'
    ))
    story.append(hr())

    # === MACRO MEMO ===
    story.append(h1('Macro Memo'))
    story.append(h2('The Consequences of Abundant Intelligence'))
    story.append(Paragraph('CitriniResearch | February 22nd, 2026 &rarr; June 30th, 2028', s_caption))
    story.append(sp(3))

    story.append(p(
        'The unemployment rate printed 10.2% this morning, a 0.3% upside surprise. The market sold off '
        '2% on the number, bringing the cumulative drawdown in the S&amp;P to 38% from its October 2026 highs.'
    ))
    story.append(p(
        'Traders have grown numb. Six months ago, a print like this would have triggered a circuit breaker.'
    ))
    story.append(p(
        'Two years. That\'s all it took to get from "contained" and "sector-specific" to an economy that '
        'no longer resembles the one any of us grew up in. This quarter\'s macro memo is our attempt to '
        'reconstruct the sequence - a post-mortem on the pre-crisis economy.'
    ))
    story.append(p(
        'The euphoria was palpable. By October 2026, the S&amp;P 500 flirted with 8000, the Nasdaq broke '
        'above 30k. The initial wave of layoffs due to human obsolescence began in early 2026, and they did '
        'exactly what layoffs are supposed to. Margins expanded, earnings beat, stocks rallied. Record-setting '
        'corporate profits were funneled right back into AI compute.'
    ))
    story.append(p(
        'The headline numbers were still great. Nominal GDP repeatedly printed mid-to-high single-digit '
        'annualized growth. Productivity was booming. Real output per hour rose at rates not seen since the '
        '1950s, driven by AI agents that don\'t sleep, take sick days or require health insurance.'
    ))
    story.append(p(
        'The owners of compute saw their wealth explode as labor costs vanished. Meanwhile, real wage growth '
        'collapsed. Despite the administration\'s repeated boasts of record productivity, white-collar workers '
        'lost jobs to machines and were forced into lower-paying roles.'
    ))
    story.append(p(
        'When cracks began appearing in the consumer economy, economic pundits popularized the phrase '
        '"Ghost GDP": output that shows up in the national accounts but never circulates through the real economy.'
    ))
    story.append(p(
        'In every way AI was exceeding expectations, and the market was AI. The only problem...the economy was not.'
    ))
    story.append(p(
        'It should have been clear all along that a single GPU cluster in North Dakota generating the output '
        'previously attributed to 10,000 white-collar workers in midtown Manhattan is more economic pandemic '
        'than economic panacea. The velocity of money flatlined. The human-centric consumer economy, 70% of '
        'GDP at the time, withered. We probably could have figured this out sooner if we just asked how much '
        'money machines spend on discretionary goods. (Hint: it\'s zero.)'
    ))
    story.append(p(
        'AI capabilities improved, companies needed fewer workers, white collar layoffs increased, displaced '
        'workers spent less, margin pressure pushed firms to invest more in AI, AI capabilities improved...'
    ))
    story.append(p(
        'It was a negative feedback loop with no natural brake. The human intelligence displacement spiral. '
        'White-collar workers saw their earnings power (and, rationally, their spending) structurally impaired. '
        'Their incomes were the bedrock of the $13 trillion mortgage market - forcing underwriters to reassess '
        'whether prime mortgages are still money good.'
    ))
    story.append(p(
        'Seventeen years without a real default cycle had left privates bloated with PE-backed software deals '
        'that assumed ARR would remain recurring. The first wave of defaults due to AI disruption in mid-2027 '
        'challenged that assumption.'
    ))
    story.append(p(
        'This would have been manageable if the disruption remained contained to software, but it didn\'t. '
        'By the end of 2027, it threatened every business model predicated on intermediation. Swaths of '
        'companies built on monetizing friction for humans disintegrated.'
    ))
    story.append(p(
        'The system turned out to be one long daisy chain of correlated bets on white-collar productivity '
        'growth. The November 2027 crash only served to accelerate all of the negative feedback loops already '
        'in place.'
    ))
    story.append(p(
        'We\'ve been waiting for "bad news is good news" for almost a year now. The government is starting to '
        'consider proposals, but public faith in the ability of the government to stage any sort of rescue has '
        'dwindled. Policy response has always lagged economic reality, but lack of a comprehensive plan is now '
        'threatening to accelerate a deflationary spiral.'
    ))

    story.append(hr())

    # === HOW IT STARTED ===
    story.append(h2('How It Started'))
    story.append(p(
        'In late 2025, agentic coding tools took a step function jump in capability.'
    ))
    story.extend(img('03_chart.png', 'AI Agent Capability Scaling'))
    story.append(p(
        'A competent developer working with Claude Code or Codex could now replicate the core functionality '
        'of a mid-market SaaS product in weeks. Not perfectly or with every edge case handled, but well enough '
        'that the CIO reviewing a $500k annual renewal started asking the question "what if we just built this '
        'ourselves?"'
    ))
    story.append(p(
        'Fiscal years mostly line up with calendar years, so 2026 enterprise spend had been set in Q4 2025, '
        'when "agentic AI" was still a buzzword. The mid-year review was the first time procurement teams were '
        'making decisions with visibility into what these systems could actually do. Some watched their own '
        'internal teams spin up prototypes replicating six-figure SaaS contracts in weeks.'
    ))
    story.append(p(
        'That summer, we spoke with a procurement manager at a Fortune 500. He told us about one of his budget '
        'negotiations. The salesperson had expected to run the same playbook as last year: a 5% annual price '
        'increase, the standard "your team depends on us" pitch. The procurement manager told him he\'d been in '
        'conversations with OpenAI about having their "forward deployed engineers" use AI tools to replace the '
        'vendor entirely. They renewed at a 30% discount. That was a good outcome, he said. The "long-tail of '
        'SaaS", like Monday.com, Zapier and Asana, had it much worse.'
    ))
    story.append(p(
        'Investors were prepared - expectant, even - that the long tail would be hit hard. They may have made up '
        'a third of spending for the typical enterprise stack, but they were obviously exposed. The systems of '
        'record, however, were supposed to be safe from disruption.'
    ))
    story.append(p(
        'It wasn\'t until ServiceNow\'s Q3 26 report that the mechanism of reflexivity became clearer.'
    ))
    story.extend(quote_block(
        'SERVICENOW NET NEW ACV GROWTH DECELERATES TO 14% FROM 23%; ANNOUNCES 15% WORKFORCE REDUCTION AND '
        '\'STRUCTURAL EFFICIENCY PROGRAM\'; SHARES FALL 18% | Bloomberg, October 2026'
    ))
    story.append(p(
        'SaaS wasn\'t "dead". There was still a cost-benefit-analysis to running and supporting in-house builds. '
        'But in-house was an option, and that factored into pricing negotiations. Perhaps more importantly, the '
        'competitive landscape had changed. AI had made it easier to develop and ship new features, so '
        'differentiation collapsed. Incumbents were in a race to the bottom on pricing - a knife-fight with both '
        'each other and with the new crop of upstart challengers that popped up.'
    ))
    story.append(p(
        'The interconnected nature of these systems weren\'t fully appreciated until this print, either. '
        'ServiceNow sold seats. When Fortune 500 clients cut 15% of their workforce, they cancelled 15% of '
        'their licenses. The same AI-driven headcount reductions that were boosting margins at their customers '
        'were mechanically destroying their own revenue base.'
    ))
    story.append(p(
        'The company that sold workflow automation was being disrupted by better workflow automation, and its '
        'response was to cut headcount and use the savings to fund the very technology disrupting it.'
    ))
    story.append(p(
        'What else were they supposed to do? Sit still and die slower? The companies most threatened by AI '
        'became AI\'s most aggressive adopters.'
    ))
    story.append(p(
        'This sounds obvious in hindsight, but it really wasn\'t at the time (at least to me). The historical '
        'disruption model said incumbents resist new technology, they lose share to nimble entrants and die slowly. '
        'That\'s what happened to Kodak, to Blockbuster, to BlackBerry. What happened in 2026 was different; the '
        'incumbents didn\'t resist because they couldn\'t afford to.'
    ))
    story.append(p(
        'With stocks down 40-60% and boards demanding answers, the AI-threatened companies did the only thing they '
        'could. Cut headcount, redeploy the savings into AI tools, use those tools to maintain output with lower costs.'
    ))
    story.append(p(
        'Each company\'s individual response was rational. The collective result was catastrophic. Every dollar saved '
        'on headcount flowed into AI capability that made the next round of job cuts possible.'
    ))
    story.extend(img('04_chart.png', 'The AI Feedback Loop: A Non-Cyclical Disruption'))
    story.append(p(
        'Software was only the opening act. What investors missed while they debated whether SaaS multiples had '
        'bottomed was that the reflexive loop had already escaped the software sector. The same logic that justified '
        'ServiceNow cutting headcount applied to every company with a white-collar cost structure.'
    ))

    story.append(hr())

    # === FRICTION ===
    story.append(h2('When Friction Went to Zero'))
    story.append(p(
        'By early 2027, LLM usage had become default. People were using AI agents who didn\'t even know what an '
        'AI agent was, in the same way people who never learned what "cloud computing" was used streaming services. '
        'They thought of it the same way they thought of autocomplete or spell-check - a thing their phone just did now.'
    ))
    story.append(p(
        'Qwen\'s open-source agentic shopper was the catalyst for AI handling consumer decisions. Within weeks, every '
        'major AI assistant had integrated some agentic commerce feature. Distilled models meant these agents could '
        'run on phones and laptops, not just cloud instances, reducing the marginal cost of inference significantly.'
    ))
    story.append(p(
        'The part that should have unsettled investors more than it did was that these agents didn\'t wait to be asked. '
        'They ran in the background according to the user\'s preferences. Commerce stopped being a series of discrete '
        'human decisions and became a continuous optimization process, running 24/7 on behalf of every connected '
        'consumer. By March 2027, the median individual in the United States was consuming 400,000 tokens per day - '
        '10x since the end of 2026.'
    ))
    story.append(p('The next link in the chain was already breaking. Intermediation.'))
    story.append(p(
        'Over the past fifty years, the U.S. economy built a giant rent-extraction layer on top of human limitations: '
        'things take time, patience runs out, brand familiarity substitutes for diligence, and most people are willing '
        'to accept a bad price to avoid more clicks. Trillions of dollars of enterprise value depended on those '
        'constraints persisting.'
    ))
    story.append(p(
        'It started out simple enough. Agents removed friction. Subscriptions and memberships that passively renewed '
        'despite months of disuse. Introductory pricing that sneakily doubled after the trial period. Each one was '
        'rebranded as a hostage situation that agents could negotiate. The average customer lifetime value, the metric '
        'the entire subscription economy was built on, distinctly declined.'
    ))
    story.append(p(
        'Humans don\'t really have the time to price-match across five competing platforms before buying a box of '
        'protein bars. Machines do.'
    ))
    story.append(p(
        'Travel booking platforms were an early casualty. Insurance renewals, where the entire renewal model depended '
        'on policyholder inertia, were reformed. Financial advice. Tax prep. Routine legal work. Any category where '
        'the service provider\'s value proposition was ultimately "I will navigate complexity that you find tedious" '
        'was disrupted, as the agents found nothing tedious.'
    ))
    story.append(p(
        'Even places we thought insulated by the value of human relationships proved fragile. Real estate, where '
        'buyers had tolerated 5-6% commissions for decades, crumbled once AI agents equipped with MLS access and '
        'decades of transaction data could replicate the knowledge base instantly. A sell-side piece from March 2027 '
        'titled it "agent on agent violence". The median buy-side commission in major metros had compressed from '
        '2.5-3% to under 1%.'
    ))
    story.append(p(
        'We had overestimated the value of "human relationships". Turns out that a lot of what people called '
        'relationships was simply friction with a friendly face.'
    ))
    story.append(p(
        'DoorDash was the poster child. Coding agents had collapsed the barrier to entry for launching a delivery app. '
        'The DoorDash moat was literally "you\'re hungry, you\'re lazy, this is the app on your home screen." An agent '
        'doesn\'t have a home screen. It checks DoorDash, Uber Eats, the restaurant\'s own site, and twenty new '
        'vibe-coded alternatives so it can pick the lowest fee and fastest delivery every time.'
    ))
    story.append(p(
        'Habitual app loyalty, the entire basis of the business model, simply didn\'t exist for a machine.'
    ))
    story.append(p(
        'Once agents controlled the transaction, they went looking for bigger paperclips. In machine-to-machine '
        'commerce, the 2-3% card interchange rate became an obvious target. Agents settled on using stablecoins via '
        'Solana or Ethereum L2s, where settlement was near-instant and the transaction cost was measured in fractions '
        'of a penny.'
    ))
    story.extend(img('02_chart.png', 'How Payment Rails are Changing'))
    story.extend(quote_block(
        'MASTERCARD Q1 2027: NET REVENUES +6% Y/Y; PURCHASE VOLUME GROWTH SLOWS TO +3.4% Y/Y FROM +5.9% PRIOR '
        'QUARTER; MANAGEMENT NOTES "AGENT-LED PRICE OPTIMIZATION" AND "PRESSURE IN DISCRETIONARY CATEGORIES" '
        '| Bloomberg, April 29 2027'
    ))
    story.append(p(
        'Mastercard\'s Q1 2027 report was the point of no return. Agentic commerce went from being a product story '
        'to a plumbing story. American Express was hit hardest; a combined headwind from white-collar workforce '
        'reductions gutting its customer base and agents routing around interchange gutting its revenue model.'
    ))
    story.append(p('Their moats were made of friction. And friction was going to zero.'))

    story.append(hr())

    # === SECTOR TO SYSTEMIC ===
    story.append(h2('From Sector Risk to Systemic Risk'))
    story.append(p(
        'Through 2026, markets treated negative AI impact as a sector story. Software and consulting were getting '
        'crushed, payments and other toll booths were wobbly, but the broader economy seemed fine. The consensus view '
        'was that creative destruction was part of any technological innovation cycle.'
    ))
    story.append(p(
        'Our January 2027 macro memo argued this was the wrong mental model. The US economy is a white-collar '
        'services economy. White-collar workers represented 50% of employment and drove roughly 75% of discretionary '
        'consumer spending. The businesses and jobs that AI was chewing up were not tangential to the US economy, '
        'they were the US economy.'
    ))
    story.append(p(
        '"Technological innovation destroys jobs and then creates even more". This was the most popular and convincing '
        'counter-argument at the time. It was popular and convincing because it\'d been right for two centuries.'
    ))
    story.append(p(
        'ATMs made branches cheaper to operate so banks opened more of them and teller employment rose for the next '
        'twenty years. The internet disrupted travel agencies, the Yellow Pages, brick-and-mortar retail, but it '
        'invented entirely new industries in their place that conjured new jobs.'
    ))
    story.append(p(
        'Every new job, however, required a human to perform it.'
    ))
    story.append(p(
        'AI is now a general intelligence that improves at the very tasks humans would redeploy to. Displaced coders '
        'cannot simply move to "AI management" because AI is already capable of that.'
    ))
    story.append(p(
        'AI has created new jobs. Prompt engineers. AI safety researchers. Infrastructure technicians. For every new '
        'role AI created, though, it rendered dozens obsolete. The new roles paid a fraction of what the old ones did.'
    ))
    story.extend(quote_block(
        'U.S. JOLTS: JOB OPENINGS FALL BELOW 5.5M; UNEMPLOYED-TO-OPENINGS RATIO CLIMBS TO ~1.7, HIGHEST '
        'SINCE AUG 2020 | Bloomberg, Oct 2026'
    ))
    story.append(p(
        'White-collar openings were collapsing while blue-collar openings remained relatively stable. The equity '
        'market still cared less about JOLTS than it did the news that all of GE Vernova\'s turbine capacity was '
        'now sold out until 2040. The bond market began pricing the consumption hit, however. The 10-year yield '
        'began a descent from 4.3% to 3.2% over the following four months.'
    ))
    story.append(p(
        'In a normal recession, the cause eventually self-corrects. This cycle\'s cause was not cyclical.'
    ))
    story.append(p(
        'AI got better and cheaper. Companies laid off workers, then used the savings to buy more AI capability, '
        'which let them lay off more workers. Displaced workers spent less. A feedback loop with no natural brake.'
    ))
    story.append(p(
        'The intuitive expectation was that falling aggregate demand would slow the AI buildout. It didn\'t, because '
        'this wasn\'t hyperscaler-style CapEx. It was OpEx substitution. A company that had been spending $100M a year '
        'on employees and $5M on AI now spent $70M on employees and $20M on AI. Every company\'s AI budget grew while '
        'its overall spending shrank.'
    ))
    story.append(p(
        'The irony was that the AI infrastructure complex kept performing even as the economy it was disrupting began '
        'deteriorating. NVDA was still posting record revenues. TSM was still running at 95%+ utilization. Economies '
        'purely convex to this trend, like Taiwan and Korea, outperformed massively.'
    ))
    story.append(p(
        'India was the inverse. The country\'s IT services sector exported over $200 billion annually. The entire model '
        'was built on one value proposition: Indian developers cost a fraction of their American counterparts. But the '
        'marginal cost of an AI coding agent had collapsed to, essentially, the cost of electricity. The rupee fell '
        '18% against the dollar in four months. By Q1 2028, the IMF had begun "preliminary discussions" with New Delhi.'
    ))

    story.append(hr())

    # === DISPLACEMENT SPIRAL ===
    story.append(h2('The Intelligence Displacement Spiral'))
    story.append(p(
        '2027 was when the macroeconomic story stopped being subtle. The transmission mechanism from the previous '
        'twelve months of disjointed but clearly negative developments became obvious. You didn\'t need to go into '
        'the BLS data. Just attend a dinner party with friends.'
    ))
    story.extend(img('05_chart.jpeg', 'The Great Displacement: Tech Workforce to Gig Economy Flow'))
    story.append(p(
        'Displaced white-collar workers did not sit idle. They downshifted. Many took lower-paying service sector '
        'and gig economy jobs, which increased labor supply in those segments and compressed wages there too.'
    ))
    story.append(p(
        'A friend of ours was a senior product manager at Salesforce in 2025. Title, health insurance, 401k, '
        '$180,000 a year. She lost her job in the third round of layoffs. After six months of searching, she started '
        'driving for Uber. Her earnings dropped to $45,000. The point is less the individual story and more the '
        'second-order math. Multiply this dynamic by a few hundred thousand workers across every major metro. '
        'Sector-specific disruption metastasized into economy-wide wage compression.'
    ))
    story.append(p(
        'By February 2027, it was clear that still employed professionals were spending like they might be next. '
        'Savings rates ticked higher and spending softened.'
    ))
    story.extend(quote_block(
        'U.S. INITIAL JOBLESS CLAIMS SURGE TO 487,000, HIGHEST SINCE APRIL 2020; Department of Labor, Q3 2027'
    ))
    story.append(p(
        'The S&amp;P dropped 6% over the following week. In this cycle, the job losses have been concentrated in '
        'the upper deciles of the income distribution. The top 10% of earners account for more than 50% of all '
        'consumer spending in the United States. The top 20% account for roughly 65%. These are the people who buy '
        'the houses, the cars, the vacations, the restaurant meals. They are the demand base for the entire consumer '
        'discretionary economy.'
    ))
    story.append(p(
        'By Q2 2027, the economy was in recession. But it wasn\'t a "financial crisis"...yet.'
    ))

    story.append(hr())

    # === DAISY CHAIN ===
    story.append(h2('The Daisy Chain of Correlated Bets'))
    story.append(p(
        'Private credit had grown from under $1 trillion in 2015 to over $2.5 trillion by 2026. A meaningful share '
        'of that capital had been deployed into software and technology deals, many of them leveraged buyouts of '
        'SaaS companies at valuations that assumed mid-teens revenue growth in perpetuity.'
    ))
    story.extend(quote_block(
        'MOODY\'S DOWNGRADES $18B OF PE-BACKED SOFTWARE DEBT ACROSS 14 ISSUERS, CITING \'SECULAR REVENUE '
        'HEADWINDS FROM AI-DRIVEN COMPETITIVE DISRUPTION\'; LARGEST SINGLE-SECTOR ACTION SINCE ENERGY IN 2015 '
        '| Moody\'s Investors Service, April 2027'
    ))
    story.append(p(
        'Software-backed loans began defaulting in Q3 2027. Zendesk was the smoking gun.'
    ))
    story.extend(quote_block(
        'ZENDESK MISSES DEBT COVENANTS AS AI-DRIVEN CUSTOMER SERVICE AUTOMATION ERODES ARR; $5B DIRECT LENDING '
        'FACILITY MARKED TO 58 CENTS; LARGEST PRIVATE CREDIT SOFTWARE DEFAULT ON RECORD | Financial Times, '
        'September 2027'
    ))
    story.append(p(
        'In 2022, Hellman &amp; Friedman and Permira had taken Zendesk private for $10.2 billion. The debt package '
        'was $5 billion in direct lending, the largest ARR-backed facility in history at the time. The loan was '
        'explicitly structured around the assumption that Zendesk\'s annual recurring revenue would remain recurring. '
        'At roughly 25x EBITDA, the leverage only made sense if it did.'
    ))
    story.append(p(
        'By mid-2027, it didn\'t. AI agents had been handling customer service autonomously for the better part of '
        'a year. The largest ARR-backed loan in history became the largest private credit software default in history.'
    ))
    story.append(p(
        'But here\'s what the consensus got right, at least initially: this should have been survivable. Private '
        'credit is not 2008 banking. The whole architecture was explicitly designed to avoid forced selling. '
        'These are closed-end vehicles with locked-up capital.'
    ))
    story.extend(img('06_chart.jpeg', 'Private Credit &amp; Life Insurers: From Flywheel to Noose'))
    story.append(p(
        'The "permanent capital" that was supposed to make the system resilient was not some abstract pool of patient '
        'institutional money. It was the savings of American households, structured as annuities invested in the same '
        'PE-backed software and technology paper that was now defaulting. The locked-up capital that couldn\'t run was '
        'life insurance policyholder money, and the rules are a bit different there.'
    ))
    story.extend(quote_block(
        'NEW YORK, IOWA STATE REGULATORS MOVE TO TIGHTEN CAPITAL TREATMENT FOR CERTAIN PRIVATELY RATED CREDIT '
        'HELD BY LIFE INSURERS; NAIC GUIDANCE EXPECTED TO INCREASE RBC FACTORS | Reuters, Nov 2027'
    ))
    story.append(p(
        'When Moody\'s put Athene\'s financial strength rating on negative outlook, Apollo\'s stock dropped 22% in '
        'two sessions. The November 2027 crash marked the transition of perception from a potentially garden-variety '
        'cyclical drawdown to something much more uncomfortable. "A daisy chain of correlated bets on white collar '
        'productivity growth" was what Fed Chair Kevin Warsh called it.'
    ))
    story.append(p(
        'It is never the losses themselves that cause the crisis. It\'s recognizing them.'
    ))

    story.append(hr())

    # === MORTGAGE ===
    story.append(h2('The Mortgage Question'))
    story.extend(img('07_chart.png', '2008 vs. 2028: Mortgage Concerns'))
    story.append(p(
        'The US residential mortgage market is approximately $13 trillion. Mortgage underwriting is built on the '
        'fundamental assumption that the borrower will remain employed at roughly their current income level for '
        'the duration of the loan. For thirty years, in the case of most mortgages.'
    ))
    story.append(p(
        'The white-collar employment crisis has threatened this assumption with a sustained shift in income '
        'expectations. We now have to ask a question that seemed absurd just 3 years ago - are prime mortgages '
        'money good?'
    ))
    story.append(p(
        'Every prior mortgage crisis in US history has been driven by one of three things: speculative excess '
        '(as in 2008), interest rate shocks (as in the early 1980s), or localized economic shocks (like oil in '
        'Texas in the 1980s). None of these apply here. The borrowers in question are not subprime. They\'re 780 '
        'FICO scores. They put 20% down. They have clean credit histories.'
    ))
    story.append(p(
        'In 2008, the loans were bad on day one. In 2028, the loans were good on day one. The world just...changed '
        'after the loans were written. People borrowed against a future they can no longer afford to believe in.'
    ))
    story.extend(img('08_chart.png', 'The Mortgage Accelerant to the Intelligence Displacement Spiral'))
    story.append(p(
        'The Intelligence Displacement Spiral now has two financial accelerants. Traditional policy tools (rate cuts, '
        'QE) can address the financial engine but cannot address the real economy engine, because the real economy '
        'engine is not driven by tight financial conditions. It\'s driven by AI making human intelligence less scarce '
        'and less valuable.'
    ))
    story.append(p(
        'You can cut rates to zero and buy every MBS and all the defaulted software LBO debt in the market... '
        'It won\'t change the fact that a Claude agent can do the work of a $180,000 product manager for $200/month.'
    ))
    story.append(p(
        'If these fears manifest, the mortgage market cracks in the back half of this year. In that scenario, we\'d '
        'expect the current drawdown in equities to ultimately rival that of the GFC (57% peak-to-trough). This would '
        'bring the S&amp;P500 to ~3500 - levels we haven\'t seen since the month before the ChatGPT moment in '
        'November 2022.'
    ))

    story.append(hr())

    # === BATTLE AGAINST TIME ===
    story.append(h2('The Battle Against Time'))
    story.extend(img('09_chart.png', 'The Three Drivers of the 2028 Global Intelligence Crisis'))
    story.append(p(
        'The system wasn\'t designed for a crisis like this. The federal government\'s revenue base is essentially a '
        'tax on human time. People work, firms pay them, the government takes a cut.'
    ))
    story.append(p(
        'Through Q1 of this year, federal receipts were running 12% below CBO baseline projections. Productivity is '
        'surging, but the gains are flowing to capital and compute, not labor.'
    ))
    story.append(p(
        'Labor\'s share of GDP declined from 64% in 1974 to 56% in 2024, a four-decade grind lower. In the four '
        'years since AI began its exponential improvement, that has dropped to 46%. The sharpest decline on record.'
    ))
    story.extend(img('10_chart.png', 'Circular Flow Diagram: Pre- vs Post-Superintelligence'))
    story.append(p(
        'The output is still there. But it\'s no longer routing through households on the way back to firms, which '
        'means it\'s no longer routing through the IRS either. The circular flow is breaking.'
    ))
    story.append(p(
        'The administration began entertaining bipartisan proposals for the "Transition Economy Act": a framework '
        'for direct transfers to displaced workers funded by deficit spending and a proposed tax on AI inference '
        'compute. The most radical proposal, the "Shared AI Prosperity Act", would establish a public claim on the '
        'returns of the intelligence infrastructure itself.'
    ))
    story.append(p(
        'While the politicians bicker, the social fabric is fraying faster than the legislative process can move. '
        'The Occupy Silicon Valley movement has been emblematic of wider dissatisfaction.'
    ))
    story.append(p(
        'Every side has their own villain, but the real villain is time. AI capability is evolving faster than '
        'institutions can adapt. If the government doesn\'t agree on what the problem is soon, the feedback loop '
        'will write the next chapter for them.'
    ))

    story.append(hr())

    # === INTELLIGENCE PREMIUM ===
    story.append(h2('The Intelligence Premium Unwind'))
    story.append(p(
        'For the entirety of modern economic history, human intelligence has been the scarce input. Capital was '
        'abundant. Natural resources were finite but substitutable. Technology improved slowly enough that humans '
        'could adapt. Intelligence - the ability to analyze, decide, create, persuade, and coordinate - was the '
        'thing that could not be replicated at scale.'
    ))
    story.append(p(
        'Human intelligence derived its inherent premium from its scarcity. Every institution in our economy, from '
        'the labor market to the mortgage market to the tax code, was designed for a world in which that assumption held.'
    ))
    story.append(p(
        'We are now experiencing the unwind of that premium. Machine intelligence is now a competent and rapidly '
        'improving substitute for human intelligence across a growing range of tasks. The financial system, optimized '
        'over decades for a world of scarce human minds, is repricing. That repricing is painful, disorderly, and '
        'far from complete.'
    ))
    story.append(p('But repricing is not the same as collapse.'))
    story.append(p(
        'The economy can find a new equilibrium. Getting there is one of the few tasks left that only humans can do. '
        'We need to do it correctly.'
    ))
    story.append(p(
        'This is the first time in history the most productive asset in the economy has produced fewer, not more, '
        'jobs. Nobody\'s framework fits, because none were designed for a world where the scarce input became abundant. '
        'So we have to make new frameworks. Whether we build them in time is the only question that matters.'
    ))

    story.append(hr())

    # === CODA ===
    story.append(sp(5))
    story.append(Paragraph(
        'But you\'re not reading this in June 2028. You\'re reading it in February 2026.',
        ParagraphStyle('Coda', parent=s_body, fontName=FONT_SERIF_I, fontSize=12, leading=22,
                       textColor=C_BLACK, alignment=TA_CENTER, spaceBefore=5*mm, spaceAfter=5*mm)
    ))
    story.append(sp(3))
    story.append(p(
        'The S&amp;P is near all-time highs. The negative feedback loops have not begun. We are certain some of '
        'these scenarios won\'t materialize. We\'re equally certain that machine intelligence will continue to '
        'accelerate. The premium on human intelligence will narrow.'
    ))
    story.append(p(
        'As investors, we still have time to assess how much of our portfolios are built upon assumptions that '
        'won\'t survive the decade. As a society, we still have time to be proactive.'
    ))
    story.append(sp(5))
    story.append(Paragraph(
        'The canary is still alive.',
        ParagraphStyle('Final', parent=s_body, fontName=FONT_SERIF_I, fontSize=14, leading=22,
                       textColor=C_BLACK, alignment=TA_CENTER, spaceBefore=5*mm, spaceAfter=10*mm)
    ))

    story.append(hr())
    story.append(sp(5))
    story.append(Paragraph(
        'Citrini Research &amp; Alap Shah | citriniresearch.com/p/2028gic | February 22, 2026',
        s_caption
    ))

    doc.build(story, onFirstPage=add_page_number, onLaterPages=add_page_number)
    print(f"PDF generated: {output_path}")
    return output_path


if __name__ == '__main__':
    build()
