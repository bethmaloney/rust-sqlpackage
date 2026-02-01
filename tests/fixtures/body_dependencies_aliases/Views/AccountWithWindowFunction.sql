-- View with window functions using aliases
-- Tests alias resolution in OVER clauses
CREATE VIEW [dbo].[AccountWithWindowFunction]
AS
SELECT
    A.Id,
    A.AccountNumber,
    T.[Name] AS TagName,
    ROW_NUMBER() OVER (PARTITION BY A.Id ORDER BY T.[Name]) AS TagRank,
    COUNT(*) OVER (PARTITION BY A.Id) AS TotalTags,
    FIRST_VALUE(T.[Name]) OVER (PARTITION BY A.Id ORDER BY T.[Name]) AS FirstTag
FROM [dbo].[Account] A
INNER JOIN [dbo].[AccountTag] AT ON AT.AccountId = A.Id
INNER JOIN [dbo].[Tag] T ON T.Id = AT.TagId;
GO
