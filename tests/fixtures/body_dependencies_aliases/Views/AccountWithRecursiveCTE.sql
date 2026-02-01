-- View with recursive CTE
-- Tests that recursive CTE references don't appear as dependencies
CREATE VIEW [dbo].[AccountWithRecursiveCTE]
AS
WITH TagHierarchy AS (
    -- Anchor member
    SELECT
        T.Id,
        T.[Name],
        CAST(T.[Name] AS NVARCHAR(MAX)) AS Path,
        0 AS Level
    FROM [dbo].[Tag] T
    WHERE T.Id = 1

    UNION ALL

    -- Recursive member - references TagHierarchy
    SELECT
        T2.Id,
        T2.[Name],
        CAST(TH.Path + ' > ' + T2.[Name] AS NVARCHAR(MAX)) AS Path,
        TH.Level + 1 AS Level
    FROM [dbo].[Tag] T2
    INNER JOIN TagHierarchy TH ON T2.Id = TH.Id + 1
    WHERE TH.Level < 10
)
SELECT
    A.Id,
    A.AccountNumber,
    TH.[Name] AS TagName,
    TH.Path AS TagPath
FROM [dbo].[Account] A
INNER JOIN [dbo].[AccountTag] AT ON AT.AccountId = A.Id
INNER JOIN TagHierarchy TH ON TH.Id = AT.TagId;
GO
