-- Scalar function with CTE to test DynamicObjects emission (Phase 24.4.1)
CREATE FUNCTION [dbo].[GetAccountWithCteFunction]
(
    @AccountId INT
)
RETURNS NVARCHAR(MAX)
AS
BEGIN
    DECLARE @Result NVARCHAR(MAX);

    WITH AccountTags AS (
        SELECT
            [T].[Name] AS TagName
        FROM [dbo].[AccountTag] [AT]
        INNER JOIN [dbo].[Tag] [T] ON [AT].TagId = [T].Id
        WHERE [AT].AccountId = @AccountId
    )
    SELECT @Result = STRING_AGG(TagName, ', ')
    FROM AccountTags;

    RETURN @Result;
END;
GO
