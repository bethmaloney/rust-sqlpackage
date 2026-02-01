-- Procedure with MERGE statement containing aliases
-- Tests alias resolution in MERGE statements
CREATE PROCEDURE [dbo].[MergeAccountTags]
    @AccountId INT,
    @TagId INT
AS
BEGIN
    SET NOCOUNT ON;

    MERGE INTO [dbo].[AccountTag] AS [TARGET]
    USING (
        SELECT
            A.Id AS AccountId,
            T.Id AS TagId
        FROM [dbo].[Account] A
        CROSS JOIN [dbo].[Tag] T
        WHERE A.Id = @AccountId AND T.Id = @TagId
    ) AS [SOURCE]
    ON [TARGET].AccountId = [SOURCE].AccountId
        AND [TARGET].TagId = [SOURCE].TagId
    WHEN NOT MATCHED BY TARGET THEN
        INSERT (AccountId, TagId)
        VALUES ([SOURCE].AccountId, [SOURCE].TagId)
    WHEN MATCHED THEN
        UPDATE SET [TARGET].AccountId = [SOURCE].AccountId;
END;
GO
