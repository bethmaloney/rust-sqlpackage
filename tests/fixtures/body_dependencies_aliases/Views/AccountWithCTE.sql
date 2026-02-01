-- View with CTE containing table aliases
-- Tests that CTE names are treated as subquery aliases, not table references
CREATE VIEW [dbo].[AccountWithCTE]
AS
WITH TaggedAccounts AS (
    -- CTE with aliases ACCT and TG
    SELECT
        [ACCT].Id,
        [ACCT].AccountNumber,
        [TG].[Name] AS TagName
    FROM [dbo].[Account] [ACCT]
    INNER JOIN [dbo].[AccountTag] [ATA] ON [ATA].AccountId = [ACCT].Id
    INNER JOIN [dbo].[Tag] [TG] ON [TG].Id = [ATA].TagId
)
SELECT
    Id,
    AccountNumber,
    TagName
FROM TaggedAccounts
WHERE TagName IS NOT NULL;
GO
