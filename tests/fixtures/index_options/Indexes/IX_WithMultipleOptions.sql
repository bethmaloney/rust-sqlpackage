-- Index with multiple options
CREATE NONCLUSTERED INDEX [IX_LargeTable_CreatedAt_MultiOption]
ON [dbo].[LargeTable] ([CreatedAt] DESC)
INCLUDE ([Name], [Category])
WITH (
    FILLFACTOR = 90,
    PAD_INDEX = ON,
    SORT_IN_TEMPDB = ON,
    IGNORE_DUP_KEY = OFF,
    STATISTICS_NORECOMPUTE = OFF,
    ALLOW_ROW_LOCKS = ON,
    ALLOW_PAGE_LOCKS = ON
);
GO
